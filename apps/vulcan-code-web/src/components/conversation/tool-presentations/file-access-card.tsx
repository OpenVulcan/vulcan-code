// File access tool card for reads, listings and searches.
// 文件访问工具卡，覆盖读取、列目录与搜索。
import * as React from "react";
import { ChevronDown, ChevronRight, FileText, FolderSearch, Search } from "lucide-react";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { FileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { hydrateFileAccessPresentation, type FileAccessItemPresentation, type FileAccessPresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { ToolResultEventContext, ToolResultBody, type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel, fileTargetLabel, FileTextFormatBadge, FileChangeErrorDetail } from "./tool-card-common";

// fileAccessPresentationDetailCache avoids refetching native read-only results after ordinary remounts.
// fileAccessPresentationDetailCache 避免原生只读结果在普通重挂后重复请求。
const fileAccessPresentationDetailCache = new Map<string, FileAccessPresentation>();

// FileAccessPresentationCard renders native read-only file actions without mutation semantics.
// FileAccessPresentationCard 渲染不含变更语义的原生只读文件动作。
export function FileAccessPresentationCard(props: {
  // Independent read-only file presentation.
  // 独立只读文件展示模型。
  presentation: FileAccessPresentation;
  // Whether complete returned content is expanded.
  // 完整返回内容是否展开。
  expanded: boolean;
  // Optional request-formation state.
  // 可选请求构造状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates top-level expansion state.
  // 更新顶层展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one complete hidden terminal result.
  // 加载一份完整隐藏终态返回。
  onLoadToolDetail?: ToolDetailLoader;
}) {
  // t resolves action-specific card titles and transient state copy.
  // t 解析动作专用卡片标题与瞬时状态文案。
  const { t } = useI18n();
  // detailCacheKey binds one terminal detail to this exact invocation.
  // detailCacheKey 将一条终态详情绑定到当前精确调用。
  const detailCacheKey = fileAccessPresentationDetailCacheKey(props.presentation);
  // hydratedPresentation contains only authoritative returned rows.
  // hydratedPresentation 仅包含权威返回行。
  const [hydratedPresentation, setHydratedPresentation] = React.useState<FileAccessPresentation>(
    () => detailCacheKey
      ? fileAccessPresentationDetailCache.get(detailCacheKey) ?? props.presentation
      : props.presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey
      ? fileAccessPresentationDetailCache.get(detailCacheKey)
      : undefined;
    setHydratedPresentation(cached ?? props.presentation);
  }, [detailCacheKey, props.presentation]);
  React.useEffect(() => {
    if (
      !props.onLoadToolDetail
      || !props.presentation.detailRef
      || !detailCacheKey
      || fileAccessPresentationDetailCache.has(detailCacheKey)
    ) {
      return;
    }
    // cancelled prevents a stale detail response from replacing a rebound card.
    // cancelled 防止过期详情响应替换已经重新绑定的卡片。
    let cancelled = false;
    void props.onLoadToolDetail(props.presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateFileAccessPresentation(props.presentation, payload);
        fileAccessPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, props.onLoadToolDetail, props.presentation]);
  // canExpand remains false until complete returned rows are locally available.
  // canExpand 在完整返回行已在本地可用前始终为 false。
  const resultEvent = React.useContext(ToolResultEventContext);
  const canExpand = hydratedPresentation.markdownBody !== undefined || (resultEvent !== undefined && hydratedPresentation.action !== "read") || (hydratedPresentation.interactive
    && hydratedPresentation.items.some((item) => item.expandable));
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  // title is selected only from the exact native action.
  // title 仅根据精确原生动作选择。
  const title = nativeFileAccessLabel(hydratedPresentation.action, t);
  // targetLabel exposes the actual target without edit counts or change wording.
  // targetLabel 展示实际目标，不包含编辑计数或变更文案。
  const targetLabel = hydratedPresentation.items
    .map((item) => fileTargetLabel(item.displayPath, item.lineRanges, t))
    .join(", ");
  // handleToggle opens only complete terminal returned content.
  // handleToggle 仅打开完整终态返回内容。
  const handleToggle = React.useCallback(() => {
    if (!canExpand) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(hydratedPresentation.id, !props.expanded);
  }, [canExpand, hydratedPresentation.id, props]);
  return (
    <div className={cn("tool-presentation-card file-access", canExpand && props.expanded && "expanded")}>
      <button
        aria-disabled={!canExpand}
        aria-expanded={canExpand && props.expanded}
        className="tool-presentation-summary file-access-summary"
        onClick={handleToggle}
        title={targetLabel}
        type="button"
      >
        <FileAccessOperationIcon action={hydratedPresentation.action} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="file-access-target"><span className="file-operation-label">{title}</span>
            {hydratedPresentation.action === "read" && hydratedPresentation.items.length === 1 ? <FileTypeIcon icon={hydratedPresentation.items[0].icon} /> : null}
            <span className="tool-presentation-command-text">{targetLabel ? ` · ${targetLabel}` : ""}</span></span>
        </span>
        {canExpand ? (
          props.expanded
            ? <ChevronDown className="tool-presentation-toggle" size={15} />
            : <ChevronRight className="tool-presentation-toggle" size={15} />
        ) : null}
      </button>
      <ConversationMeasuredCollapse collapseId={`file-access:${hydratedPresentation.id}`} expanded={canExpand && props.expanded}>
        <ToolResultBody>
        {hydratedPresentation.markdownBody !== undefined ? <MarkdownMessage content={hydratedPresentation.markdownBody} eager /> : <div className="file-access-detail">
          {hydratedPresentation.errorDetail ? <FileChangeErrorDetail detail={hydratedPresentation.errorDetail} /> : null}
          {hydratedPresentation.items.map((item) => (
            <FileAccessItem
              action={hydratedPresentation.action}
              item={item}
              key={item.id}
              showPath={hydratedPresentation.items.length > 1}
              t={t}
            />
          ))}
        </div>}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// FileAccessItem renders one complete read-only target result inside an expanded card.
// FileAccessItem 在展开卡片内渲染一个完整只读目标结果。
function FileAccessItem(props: {
  // Exact native read-only action.
  // 精确原生只读动作。
  action: FileAccessPresentation["action"];
  // Returned target item.
  // 返回的目标条目。
  item: FileAccessItemPresentation;
  // Whether a target heading is needed for a multi-target result.
  // 多目标返回是否需要目标标题。
  showPath: boolean;
  // Translates stable file metadata labels.
  // 翻译稳定文件元数据文案。
  t: (key: TranslationKey) => string;
}) {
  // rowText preserves returned order and explicit line numbers outside source-read gutters.
  // rowText 在源码读取行号槽之外保留返回顺序与明确行号。
  const rowText = props.item.rows
    .map((row) => row.lineNumber === null ? row.content : `${row.lineNumber}: ${row.content}`)
    .join("\n");
  return (
    <section className="file-access-item">
      {props.showPath ? (
        <div className="file-access-item-heading">
          <FileTypeIcon icon={props.item.icon} />
          <span>{props.item.displayPath}</span>
          <FileTextFormatBadge file={props.item} t={props.t} />
        </div>
      ) : null}
      {props.action === "read" && props.item.diffFile ? (
        <ChangeSetDiffPreview
          file={props.item.diffFile}
          hunkSeparatorStyle="simple"
          inlineDiff={false}
        />
      ) : rowText ? (
        <pre className="file-access-output">{rowText}</pre>
      ) : null}
    </section>
  );
}

// FileAccessOperationIcon renders an action-specific read-only icon without edit affordances.
// FileAccessOperationIcon 渲染动作专用只读图标，不包含编辑暗示。
function FileAccessOperationIcon(props: {
  // Exact native read-only action.
  // 精确原生只读动作。
  action: FileAccessPresentation["action"];
}) {
  if (props.action === "list") {
    return <FolderSearch className="file-access-operation-icon" size={15} />;
  }
  if (props.action === "search") {
    return <Search className="file-access-operation-icon" size={15} />;
  }
  return <FileText className="file-access-operation-icon" size={15} />;
}

// fileAccessPresentationDetailCacheKey returns a stable cache key for one native read-only detail.
// fileAccessPresentationDetailCacheKey 返回单条原生只读详情的稳定缓存键。
function fileAccessPresentationDetailCacheKey(
  presentation: FileAccessPresentation,
): string | null {
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

// nativeFileAccessLabel maps read-only native actions to existing localized shell labels.
// nativeFileAccessLabel 将原生只读动作映射到已有的本地化外壳文案。
function nativeFileAccessLabel(
  action: FileAccessPresentation["action"],
  t: (key: TranslationKey) => string,
): string {
  if (action === "list") {
    return t("toolPresentation.fileList.title");
  }
  if (action === "search") {
    return t("toolPresentation.fileSearch.title");
  }
  if (action === "stat") {
    return t("toolPresentation.fileStat.title");
  }
  return t("toolPresentation.fileRead.title");
}
