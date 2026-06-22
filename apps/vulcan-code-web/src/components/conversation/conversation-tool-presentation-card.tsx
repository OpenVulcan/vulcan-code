import * as React from "react";
import {
  ChevronDown,
  ChevronRight,
  Pencil,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { FileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import {
  terminalStateClass,
  terminalStateLabel,
} from "@/components/conversation/conversation-terminal-model";
import {
  commandPresentationStatusLabel,
  fileChangeStatusLabel,
  hydrateCommandExecutionPresentation,
  type CommandExecutionPresentation,
  type FileChangeFilePresentation,
  type FileChangeOperation,
  type FileChangePresentation,
  type ToolPresentation,
} from "@/components/conversation/conversation-tool-presentations";
import { cn } from "@/lib/utils";

// ToolPresentationCard renders one recognized tool presentation with shared expansion behavior.
// ToolPresentationCard 使用共享展开行为渲染单个已识别工具展示。
export function ToolPresentationCard(props: {
  // Presentation model resolved by the adapter layer.
  // 由适配器层解析出的展示模型。
  presentation: ToolPresentation;
  // Whether the card is expanded in the current transcript surface.
  // 当前转录界面中该卡片是否已展开。
  expanded: boolean;
  // Expansion id set shared with the virtualized transcript surface.
  // 与虚拟滚动转录界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for rich presentation data.
  // 加载单条隐藏工具详情记录用于补充展示数据。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  if (props.presentation.kind === "command_execution") {
    return (
      <CommandExecutionPresentationCard
        expanded={props.expanded}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
      />
    );
  }
  if (props.presentation.kind === "file_change") {
    return (
      <FileChangePresentationCard
        expanded={props.expanded}
        expandedToolPresentationIds={props.expandedToolPresentationIds}
        presentation={props.presentation}
        onExpandedChange={props.onExpandedChange}
      />
    );
  }
  return null;
}

// commandPresentationDetailCache avoids refetching hidden details after virtualized remounts.
// commandPresentationDetailCache 避免虚拟滚动重挂后重复请求隐藏详情。
const commandPresentationDetailCache = new Map<string, CommandExecutionPresentation>();

// CommandExecutionPresentationCard renders the command-line specific compact and expanded states.
// CommandExecutionPresentationCard 渲染命令行专用的紧凑与展开状态。
function CommandExecutionPresentationCard(props: {
  // Command execution presentation model.
  // 命令执行展示模型。
  presentation: CommandExecutionPresentation;
  // Whether this command card is expanded.
  // 当前命令卡片是否展开。
  expanded: boolean;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for command output.
  // 加载单条隐藏工具详情记录用于命令输出。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
}) {
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = commandPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<CommandExecutionPresentation>(
    () => detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  const [loadingDetail, setLoadingDetail] = React.useState(false);
  React.useEffect(() => {
    const cached = detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !presentation.detailRef || !detailCacheKey || commandPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateCommandExecutionPresentation(presentation, payload);
        commandPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingDetail(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);

  const outputText = commandOutputText(hydratedPresentation);
  const exitCodeLabel = hydratedPresentation.exitCode === null ? "-" : String(hydratedPresentation.exitCode);
  const exitCodeState = commandExitCodeState(hydratedPresentation.exitCode);
  const terminalState = hydratedPresentation.terminalState;
  const statusLabel = hydratedPresentation.commandSessionAction
    ? commandSessionActionStatusLabel(hydratedPresentation.commandSessionAction, hydratedPresentation.state)
    : commandPresentationStatusLabel(hydratedPresentation.state);
  const sessionMetaLabel = commandSessionMetaLabel(hydratedPresentation);
  const detailHeading = commandDetailHeading(hydratedPresentation);
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);

  return (
    <div className={cn("tool-presentation-card", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary"
        onClick={handleToggle}
        title={presentation.command}
        type="button"
      >
        <TerminalSquare className={cn("terminal-session-icon", terminalState && terminalStateClass(terminalState))} size={15} />
        <span className="tool-presentation-state">{statusLabel}</span>
        <span className="tool-presentation-command">{hydratedPresentation.command}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`command-detail:${presentation.id}`} expanded={expanded}>
        <div className="tool-presentation-detail">
          <pre className="tool-presentation-command-line"><span>{detailHeading}</span> {commandDetailText(hydratedPresentation)}</pre>
          {hydratedPresentation.cwd || hydratedPresentation.shellLabel ? (
            <div className="tool-presentation-terminal-meta">
              {hydratedPresentation.shellLabel ? <span>{hydratedPresentation.shellLabel}</span> : null}
              {hydratedPresentation.cwd ? <span>{hydratedPresentation.cwd}</span> : null}
            </div>
          ) : null}
          <div className="tool-presentation-output-scroll">
            <TerminalOutputView
              className="tool-presentation-output-text"
              emptyText={loadingDetail ? "正在加载输出..." : "无输出"}
              text={outputText}
            />
          </div>
          {hydratedPresentation.failureReason ? (
            <div className="tool-presentation-terminal-error">{hydratedPresentation.failureReason}</div>
          ) : null}
          <div className="tool-presentation-footer">
            <span>{sessionMetaLabel}</span>
            {hydratedPresentation.exitCode !== null ? (
              <span className={cn("tool-presentation-exit-code", exitCodeState)}>退出代码 {exitCodeLabel}</span>
            ) : null}
          </div>
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// commandSessionActionStatusLabel returns an action result label instead of the session lifecycle label.
// commandSessionActionStatusLabel 返回动作结果文案，而不是会话生命周期文案。
function commandSessionActionStatusLabel(action: string, state: CommandExecutionPresentation["state"]): string {
  if (state === "failed") {
    if (action === "start") return "启动失败";
    if (action === "read") return "读取失败";
    if (action === "send") return "发送失败";
    if (action === "write") return "写入失败";
    if (action === "stop") return "停止失败";
    return "执行失败";
  }
  if (state === "running") {
    if (action === "start") return "启动中";
    if (action === "read") return "读取中";
    if (action === "send") return "发送中";
    if (action === "write") return "写入中";
    if (action === "stop") return "停止中";
    return "执行中";
  }
  if (action === "start") return "已启动";
  if (action === "list") return "已列出";
  if (action === "read") return "已读取";
  if (action === "send") return "已发送";
  if (action === "write") return "已写入";
  if (action === "stop") return "已停止";
  return commandPresentationStatusLabel(state);
}

// commandDetailHeading names the expanded content by command-session action.
// commandDetailHeading 根据命令会话动作命名展开内容。
function commandDetailHeading(presentation: CommandExecutionPresentation): string {
  if (presentation.commandSessionAction === "start") return "启动命令";
  if (presentation.commandSessionAction === "read") return "读取输出";
  if (presentation.commandSessionAction === "send") return "发送内容";
  if (presentation.commandSessionAction === "write") return "写入内容";
  if (presentation.commandSessionAction === "stop") return "停止会话";
  return "$";
}

// commandDetailText returns the primary text shown on the first expanded line.
// commandDetailText 返回展开区首行展示的主要文本。
function commandDetailText(presentation: CommandExecutionPresentation): string {
  if (presentation.commandSessionAction === "write" || presentation.commandSessionAction === "send") {
    return presentation.commandSessionInput?.trim() || presentation.command;
  }
  return presentation.command;
}

// commandSessionMetaLabel builds the footer text without making old actions look failed.
// commandSessionMetaLabel 构建页脚文本，避免让历史动作看起来失败。
function commandSessionMetaLabel(presentation: CommandExecutionPresentation): string {
  if (!presentation.commandSessionId) {
    return "";
  }
  const id = presentation.commandSessionId.length > 8 ? presentation.commandSessionId.slice(0, 8) : presentation.commandSessionId;
  if (presentation.lost) {
    return `命令会话 ${id} · 实时状态已丢失`;
  }
  if (presentation.terminalState) {
    return `命令会话 ${id} · 当前${terminalStateLabel(presentation.terminalState)}`;
  }
  return `命令会话 ${id}`;
}

// FileChangePresentationCard renders compact file operation rows and lazy diff bodies.
// FileChangePresentationCard 渲染紧凑文件操作行与懒加载 diff 内容。
function FileChangePresentationCard(props: {
  // File change presentation model.
  // 文件变更展示模型。
  presentation: FileChangePresentation;
  // Whether the top-level row is expanded.
  // 顶层行是否展开。
  expanded: boolean;
  // Expansion ids shared by the conversation surface.
  // 与会话界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const {
    expanded,
    expandedToolPresentationIds,
    onExpandedChange,
    presentation,
  } = props;
  const [errorExpanded, setErrorExpanded] = React.useState(() => presentation.state === "failed" && Boolean(presentation.errorDetail));
  React.useEffect(() => {
    setErrorExpanded(presentation.state === "failed" && Boolean(presentation.errorDetail));
  }, [presentation.errorDetail, presentation.id, presentation.state]);
  const effectiveExpanded = presentation.state === "failed" && presentation.errorDetail ? errorExpanded : expanded;
  const singleFile = presentation.files.length === 1 ? presentation.files[0] : null;
  const handleTopToggle = React.useCallback(() => {
    if (!presentation.interactive) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    if (presentation.state === "failed" && presentation.errorDetail) {
      setErrorExpanded((value) => !value);
      return;
    }
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation]);

  if (singleFile) {
    return (
      <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !presentation.interactive && "disabled")}>
        <FileChangeSummaryButton
          expanded={effectiveExpanded}
          file={singleFile}
          interactive={presentation.interactive}
          presentation={presentation}
          onToggle={handleTopToggle}
        />
        <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:single`} expanded={effectiveExpanded}>
          <FileChangeExpandableBody
            errorDetail={presentation.state === "failed" ? presentation.errorDetail : null}
            file={singleFile}
          />
        </ConversationMeasuredCollapse>
      </div>
    );
  }

  return (
    <div className={cn("tool-presentation-card file-change", effectiveExpanded && "expanded", !presentation.interactive && "disabled")}>
      <button
        aria-expanded={effectiveExpanded}
        className="tool-presentation-summary file-change-summary"
        disabled={!presentation.interactive}
        onClick={handleTopToggle}
        title={presentation.files.map((file) => file.displayPath).join("\n")}
        type="button"
      >
        <FileChangeOperationIcon operation={presentation.operation} />
        <span className="tool-presentation-state">{fileChangeStatusLabel(presentation, presentation.operation)} {presentation.files.length} 个文件</span>
        <span className="file-change-path multi">
          <span className="file-change-path-text">多文件变更</span>
          <FileChangeCounts added={presentation.totalAdded} failed={presentation.state === "failed"} removed={presentation.totalRemoved} />
        </span>
        {presentation.interactive ? (effectiveExpanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
      </button>
      <ConversationMeasuredCollapse collapseId={`file-change:${presentation.id}:tree`} expanded={effectiveExpanded}>
        <div className="file-change-tree">
          {presentation.state === "failed" && presentation.errorDetail ? (
            <FileChangeErrorDetail detail={presentation.errorDetail} />
          ) : presentation.files.map((file) => {
            const fileExpanded = expandedToolPresentationIds.has(fileExpansionId(presentation, file));
            return (
              <div className="file-change-tree-item" key={file.id}>
                <FileChangeSummaryButton
                  expanded={fileExpanded}
                  file={file}
                  interactive={file.expandable}
                  presentation={presentation}
                  showOperationIcon={false}
                  showStatusLabel={false}
                  onToggle={() => {
                    if (!file.expandable) {
                      return;
                    }
                    dispatchConversationManualViewportInteraction();
                    onExpandedChange(fileExpansionId(presentation, file), !fileExpanded);
                  }}
                />
                <ConversationMeasuredCollapse collapseId={fileExpansionId(presentation, file)} expanded={fileExpanded}>
                  <FileChangeExpandableBody file={file} />
                </ConversationMeasuredCollapse>
              </div>
            );
          })}
        </div>
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
  // Whether to show the left operation icon before the status label.
  // 是否在状态文案前显示左侧操作图标。
  showOperationIcon?: boolean;
  // Whether to show the operation status label before the file name.
  // 是否在文件名前显示操作状态文案。
  showStatusLabel?: boolean;
  // Toggle handler for interactive rows.
  // 可交互行的切换处理器。
  onToggle: () => void;
}) {
  const showOperationIcon = props.showOperationIcon ?? true;
  const showStatusLabel = props.showStatusLabel ?? true;
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
      {showStatusLabel ? <span className="tool-presentation-state">{fileChangeStatusLabel(props.presentation, props.file.operation)}</span> : null}
      <span className="file-change-path">
        <span className="file-change-icon-stack"><FileTypeIcon icon={props.file.icon} /></span>
        <span className="file-change-path-text">{props.file.displayPath}</span>
        <FileChangeCounts added={props.file.added} failed={props.presentation.state === "failed"} removed={props.file.removed} />
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
  // Whether an error badge should be displayed.
  // 是否展示错误徽标。
  failed: boolean;
  // Removed line count.
  // 删除行数。
  removed: number | null;
}) {
  return (
    <span className="file-change-counts">
      {typeof props.added === "number" && props.added > 0 ? <span className="file-change-added">+{props.added}</span> : null}
      {typeof props.removed === "number" && props.removed > 0 ? <span className="file-change-removed">-{props.removed}</span> : null}
      {props.failed ? <span className="file-change-error-badge">ERR</span> : null}
    </span>
  );
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
}) {
  if (props.errorDetail) {
    return <FileChangeErrorDetail detail={props.errorDetail} />;
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
        <div className="file-change-empty">无可展示内容</div>
      </div>
    </div>
  );
}

// FileChangeErrorDetail renders failed native vulcan-file details without opening the generic modal.
// FileChangeErrorDetail 渲染失败原生 vulcan-file 详情，避免打开通用弹窗。
function FileChangeErrorDetail(props: {
  // Error detail text.
  // 错误详情文本。
  detail: string;
}) {
  return (
    <div className="file-change-detail error">
      <pre className="file-change-error-text">{props.detail}</pre>
    </div>
  );
}

// fileExpansionId builds the child expansion id used by multi-file rows.
// fileExpansionId 构建多文件行使用的子级展开标识。
function fileExpansionId(presentation: FileChangePresentation, file: FileChangeFilePresentation): string {
  return `${presentation.id}:file:${file.id}`;
}

// commandOutputText combines stdout and stderr without exposing tool metadata labels.
// commandOutputText 合并 stdout 与 stderr，同时不暴露工具 metadata 标签。
function commandOutputText(presentation: CommandExecutionPresentation): string {
  return [presentation.stdout, presentation.stderr]
    .map((value) => value.trim())
    .filter(Boolean)
    .join("\n");
}

// commandExitCodeState maps exit codes to visual status classes.
// commandExitCodeState 将退出码映射为视觉状态类名。
function commandExitCodeState(exitCode: number | null): "unknown" | "success" | "failed" {
  if (exitCode === null) {
    return "unknown";
  }
  return exitCode === 0 ? "success" : "failed";
}

// commandPresentationDetailCacheKey creates a stable cache key for hidden command details.
// commandPresentationDetailCacheKey 为隐藏命令详情创建稳定缓存键。
function commandPresentationDetailCacheKey(presentation: CommandExecutionPresentation): string | null {
  const detailRef = presentation.detailRef;
  if (!detailRef) {
    return null;
  }
  return [
    detailRef.sessionId,
    detailRef.recordId ?? "",
    detailRef.toolCallId ?? "",
    detailRef.seq ?? "",
    detailRef.messageIndex ?? "",
  ].join(":");
}
