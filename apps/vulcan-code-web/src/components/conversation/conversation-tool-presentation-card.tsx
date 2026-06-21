import * as React from "react";
import {
  ChevronDown,
  ChevronRight,
  TerminalSquare,
} from "lucide-react";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { dispatchConversationLayoutWillChange } from "@/components/conversation/conversation-layout-events";
import {
  commandPresentationStatusLabel,
  hydrateCommandExecutionPresentation,
  type CommandExecutionPresentation,
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
        dispatchConversationLayoutWillChange();
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
  const handleToggle = React.useCallback(() => {
    dispatchConversationLayoutWillChange();
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
        <TerminalSquare size={15} />
        <span className="tool-presentation-state">{commandPresentationStatusLabel(hydratedPresentation.state)}</span>
        <span className="tool-presentation-command">{hydratedPresentation.command}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      {expanded ? (
        <div className="tool-presentation-detail">
          <pre className="tool-presentation-command-line"><span>$</span> {hydratedPresentation.command}</pre>
          <div className="tool-presentation-output-scroll">
            <pre className="tool-presentation-output-text">{outputText || (loadingDetail ? "正在加载输出..." : "无输出")}</pre>
          </div>
          <div className="tool-presentation-footer">
            <span />
            <span className={cn("tool-presentation-exit-code", exitCodeState)}>退出代码 {exitCodeLabel}</span>
          </div>
        </div>
      ) : null}
    </div>
  );
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
