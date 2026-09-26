// File change tool card with per-file summaries and expandable diffs.
// 文件变更工具卡，含逐文件摘要与可展开差异。
import * as React from "react";
import { ChevronDown, ChevronRight, GitBranch, Pencil, Trash2 } from "lucide-react";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { FileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { FileChangeFilePresentation, FileChangeOperation, FileChangePresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { type ToolRequestCardProgress, activeToolStatusLabel, FileTextFormatBadge, FileChangeErrorDetail, formatTranslation } from "./tool-card-common";

// FileChangePresentationCard renders compact file operation rows and lazy diff bodies.
// FileChangePresentationCard 渲染紧凑文件操作行与懒加载 diff 内容。
export function FileChangePresentationCard(props: {
  // File change presentation model.
  // 文件变更展示模型。
  presentation: FileChangePresentation;
  // Whether the top-level row is expanded.
  // 顶层行是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Expansion ids shared by the conversation surface.
  // 与会话界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    expandedToolPresentationIds,
    onExpandedChange,
    presentation,
  } = props;
  // displayPresentation is the authoritative visible host ChangeSet; hidden detail never replaces edit truth.
  // displayPresentation 是权威可见宿主 ChangeSet；隐藏详情绝不替换编辑事实。
  const displayPresentation = presentation;
  // Failures use the same user-controlled expansion state as every other result.
  // 失败结果与其他结果共用用户控制的展开状态。
  const effectiveExpanded = displayPresentation.interactive && expanded;
  const singleFile = displayPresentation.files.length === 1 ? displayPresentation.files[0] : null;
  // noEffectiveEdit identifies a terminal edit that produced no authoritative file changes.
  // noEffectiveEdit 标识未产生权威文件变更的终态编辑。
  const noEffectiveEdit = displayPresentation.hasEffectiveChanges === false;
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    displayPresentation.state,
    t("toolPresentation.execution.running"),
  );
  const handleTopToggle = React.useCallback(() => {
    if (!displayPresentation.interactive) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    onExpandedChange(displayPresentation.id, !expanded);
  }, [displayPresentation, expanded, onExpandedChange]);

  if (singleFile) {
    return (
      <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !displayPresentation.interactive && "disabled")}>
        <FileChangeSummaryButton
          expanded={effectiveExpanded}
          file={singleFile}
          interactive={displayPresentation.interactive}
          presentation={displayPresentation}
          requestProgress={props.requestProgress}
          t={t}
          onToggle={handleTopToggle}
        />
        <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:single`} expanded={effectiveExpanded}>
          <FileChangeExpandableBody
            errorDetail={displayPresentation.state === "failed" ? displayPresentation.errorDetail : null}
            file={singleFile}
            t={t}
          />
        </ConversationMeasuredCollapse>
      </div>
    );
  }

  return (
    <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !displayPresentation.interactive && "disabled")}>
      <button
        aria-expanded={effectiveExpanded}
        className="tool-presentation-summary file-change-summary"
        disabled={!displayPresentation.interactive}
        onClick={handleTopToggle}
        title={displayPresentation.files.map((file) => file.displayPath).join("\n")}
        type="button"
      >
        <FileChangeOperationIcon operation={displayPresentation.operation} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="file-change-path multi">
          <span className="file-change-path-text">{
            props.requestProgress && displayPresentation.files.length === 0
              ? props.requestProgress.toolName
              : noEffectiveEdit
              ? t("toolPresentation.file.notEdited")
              : formatTranslation(t, "toolPresentation.file.count", { count: String(displayPresentation.files.length) })
          }</span>
          <FileChangeCounts added={displayPresentation.totalAdded} presentation={displayPresentation} removed={displayPresentation.totalRemoved} t={t} />
        </span>
        {displayPresentation.interactive ? (effectiveExpanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
      </button>
      {effectiveExpanded && displayPresentation.files.length > 0 ? (
        <div className="file-change-tree">
          {displayPresentation.files.map((file) => {
            const fileExpanded = expandedToolPresentationIds.has(fileExpansionId(displayPresentation, file));
            return (
              <div className="file-change-tree-item" key={file.id}>
                <FileChangeSummaryButton
                  expanded={fileExpanded}
                  file={file}
                  interactive={effectiveExpanded && file.expandable}
                  presentation={displayPresentation}
                  t={t}
                  showOperationIcon={false}
                  showStatusLabel={false}
                  onToggle={() => {
                    if (!file.expandable) {
                      return;
                    }
                    dispatchConversationManualViewportInteraction();
                    onExpandedChange(fileExpansionId(displayPresentation, file), !fileExpanded);
                  }}
                />
                <ConversationMeasuredCollapse collapseId={fileExpansionId(displayPresentation, file)} expanded={effectiveExpanded && fileExpanded}>
                  <FileChangeExpandableBody file={file} t={t} />
                </ConversationMeasuredCollapse>
              </div>
            );
          })}
        </div>
      ) : null}
      <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:error`} expanded={effectiveExpanded && Boolean(displayPresentation.errorDetail)}>
        {displayPresentation.errorDetail ? <FileChangeErrorDetail detail={displayPresentation.errorDetail} /> : null}
      </ConversationMeasuredCollapse>
    </div>
  );
}

// FileChangeSummaryButton renders one clickable or disabled file summary row.
// FileChangeSummaryButton 渲染单条可点击或禁用的文件摘要行。
function FileChangeSummaryButton(props: {
  // Whether this row is expanded.
  // 当前行是否展开。
  expanded: boolean;
  // File row model.
  // 文件行模型。
  file: FileChangeFilePresentation;
  // Whether the row may be toggled.
  // 当前行是否允许切换。
  interactive: boolean;
  // Parent file change presentation.
  // 父级文件变更展示。
  presentation: FileChangePresentation;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
  // Whether to show the left operation icon before the status label.
  // 是否在状态文案前显示左侧操作图标。
  showOperationIcon?: boolean;
  // Whether to show the operation status label before the file name.
  // 是否在文件名前显示操作状态文案。
  showStatusLabel?: boolean;
  // Optional request stream state for the top-level file row.
  // 顶层文件行使用的可选请求流状态。
  requestProgress?: ToolRequestCardProgress | null;
  // Toggle handler for interactive rows.
  // 可交互行的切换处理器。
  onToggle: () => void;
}) {
  const showOperationIcon = props.showOperationIcon ?? true;
  const showStatusLabel = props.showStatusLabel ?? true;
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = showStatusLabel
    ? activeToolStatusLabel(
      props.requestProgress ?? null,
      props.presentation.state,
      props.t("toolPresentation.execution.running"),
    )
    : null;
  return (
    <button
      aria-expanded={props.expanded}
      className={cn(
        "tool-presentation-summary file-change-summary",
        !showOperationIcon && "without-operation-icon",
        !showOperationIcon && !showStatusLabel && "file-only",
      )}
      disabled={!props.interactive}
      onClick={props.onToggle}
      title={props.file.path}
      type="button"
    >
      {showOperationIcon ? <FileChangeOperationIcon operation={props.file.operation} /> : null}
      {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
      <span className="file-change-path">
        <span className="file-operation-label">{props.t(`toolPresentation.file.action.${props.file.operation}`)}</span>
        <span className="file-change-icon-stack"><FileTypeIcon icon={props.file.icon} /></span>
        <span className="file-change-path-text">{props.file.displayPath}</span>
        <FileTextFormatBadge file={props.file} t={props.t} />
        <FileChangeCounts added={props.file.added} presentation={props.presentation} removed={props.file.removed} t={props.t} />
      </span>
      {props.interactive ? (props.expanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
    </button>
  );
}

// FileChangeOperationIcon renders the leftmost operation icon for file writes and edits.
// FileChangeOperationIcon 渲染文件写入与编辑行最左侧的操作图标。
function FileChangeOperationIcon(props: {
  // Operation represented by the row.
  // 当前行代表的操作类型。
  operation: FileChangeOperation;
}) {
  if (props.operation === "rename") {
    return <GitBranch className="file-change-operation-icon" size={15} />;
  }
  if (props.operation === "delete") {
    return <Trash2 className="file-change-operation-icon" size={15} />;
  }
  return <Pencil className="file-change-operation-icon" size={15} />;
}

// FileChangeCounts renders added, removed, and error status tokens.
// FileChangeCounts 渲染新增、删除与错误状态标记。
function FileChangeCounts(props: {
  // Added line count.
  // 新增行数。
  added: number | null;
  // Authoritative status and diagnostic details for the failure badge.
  // 失败徽标使用的权威状态与诊断详情。
  presentation: FileChangePresentation;
  // Translates compact error labels.
  // 翻译紧凑错误标签。
  t: (key: TranslationKey) => string;
  // Removed line count.
  // 删除行数。
  removed: number | null;
}) {
  return (
    <span className="file-change-counts">
      {typeof props.added === "number" && props.added > 0 ? <span className="file-change-added">+{props.added}</span> : null}
      {typeof props.removed === "number" && props.removed > 0 ? <span className="file-change-removed">-{props.removed}</span> : null}
      {props.presentation.state === "failed" ? <span className="file-change-error-badge" title={props.presentation.errorDetail ?? undefined}>{props.t(fileChangeErrorLabelKey(props.presentation.errorCode))}</span> : null}
    </span>
  );
}

/** Maps the host FileToolError code to a compact UI label; absent or unknown codes remain generic failures.
 * 将宿主 FileToolError 错误码映射为紧凑界面标签；缺失或未知错误码保留为通用失败。
 * Parameter code is producer-owned; returns a translation key without guessing from error prose.
 * 参数 code 由产出方定义；返回翻译键，不从错误描述推测原因。 */
function fileChangeErrorLabelKey(code: string | null | undefined): TranslationKey {
  switch (code) {
    case "match_not_found":
    case "expected_lines_not_found":
    case "anchor_chain_not_found":
    case "patch_context_not_found": return "toolPresentation.file.error.notMatched";
    case "ambiguous_match":
    case "target_not_unique": return "toolPresentation.file.error.ambiguous";
    case "path_not_found": return "toolPresentation.file.error.pathMissing";
    case "path_already_exists": return "toolPresentation.file.error.pathExists";
    case "stale_version":
    case "stale_file": return "toolPresentation.file.error.stale";
    case "invalid_arguments": return "toolPresentation.file.error.invalidArguments";
    case "patch_syntax_error": return "toolPresentation.file.error.invalidPatch";
    case "write_failed": return "toolPresentation.file.error.writeFailed";
    case "transaction_failed": return "toolPresentation.file.error.rolledBack";
    case "unsupported_file_type": return "toolPresentation.file.error.unsupported";
    case "encoding_ambiguous":
    case "encoding_unrepresentable": return "toolPresentation.file.error.encoding";
    default: return "toolPresentation.file.error.failed";
  }
}

// FileChangeExpandableBody renders either an error detail or file diff rows.
// FileChangeExpandableBody 渲染错误详情或文件 diff 行。
function FileChangeExpandableBody(props: {
  // Optional error detail replacing normal diff rows.
  // 替代普通 diff 行的可选错误详情。
  errorDetail?: string | null;
  // File row model.
  // 文件行模型。
  file: FileChangeFilePresentation;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  if (props.errorDetail) {
    return <FileChangeErrorDetail detail={props.errorDetail} />;
  }
  if (props.file.readOnly && props.file.diffFile) {
    return (
      <div className="file-change-detail review file-change-read-preview">
        <ChangeSetDiffPreview
          file={props.file.diffFile}
          hunkSeparatorStyle="simple"
          inlineDiff={false}
        />
      </div>
    );
  }
  if (props.file.readOnly && props.file.rows.length) {
    return <FileChangeErrorDetail detail={props.file.rows.map((row) => row.content).join("\n")} />;
  }
  if (props.file.diffFile) {
    return (
      <div className="file-change-detail review">
        <ChangeSetDiffPreview file={props.file.diffFile} />
      </div>
    );
  }
  return (
    <div className="file-change-detail">
      <div className="file-change-code-scroll">
        <div className="file-change-empty">{props.t("toolPresentation.file.empty")}</div>
      </div>
    </div>
  );
}

// fileExpansionId builds the child expansion id used by multi-file rows.
// fileExpansionId 构建多文件行使用的子级展开标识。
function fileExpansionId(presentation: FileChangePresentation, file: FileChangeFilePresentation): string {
  return `${presentation.id}:file:${file.id}`;
}
