// Command execution tool card for shell commands and command sessions.
// 命令执行工具卡，覆盖 shell 命令与命令会话。
import * as React from "react";
import { ChevronDown, ChevronRight, TerminalSquare } from "lucide-react";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import { terminalStateClass, type TerminalVisualState } from "@/components/conversation/conversation-terminal-model";
import { hydrateCommandExecutionPresentation, type CommandExecutionPresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel, formatTranslation } from "./tool-card-common";

// commandPresentationDetailCache avoids refetching hidden details after ordinary remounts.
// commandPresentationDetailCache 避免普通重新挂载后重复请求隐藏详情。
const commandPresentationDetailCache = new Map<string, CommandExecutionPresentation>();

// CommandExecutionPresentationCard renders the command-line specific compact and expanded states.
// CommandExecutionPresentationCard 渲染命令行专用的紧凑与展开状态。
export function CommandExecutionPresentationCard(props: {
  // Command execution presentation model.
  // 命令执行展示模型。
  presentation: CommandExecutionPresentation;
  // Whether this command card is expanded.
  // 当前命令卡片是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for command output.
  // 加载单条隐藏工具详情记录用于命令输出。
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
  const detailCacheKey = commandPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<CommandExecutionPresentation>(
    () => detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? commandPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || commandPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
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
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);

  const outputText = commandOutputText(hydratedPresentation);
  const exitCodeLabel = hydratedPresentation.exitCode === null ? "-" : String(hydratedPresentation.exitCode);
  const exitCodeState = commandExitCodeState(hydratedPresentation.exitCode);
  const terminalState = hydratedPresentation.terminalState;
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  const sessionMetaLabel = commandSessionMetaLabel(hydratedPresentation, t);
  const detailHeading = commandDetailHeading(hydratedPresentation, t);
  const detailCommandText = commandDetailText(hydratedPresentation);
  // pluginContributions preserves every badge contributed by the ordered hook chain.
  // pluginContributions 保留有序 Hook 链贡献的全部徽标。
  const pluginContributions = hydratedPresentation.pluginContributions ?? [];
  // displayCommand keeps request-phase command cards semantic before arguments are assembled.
  // displayCommand 在参数组装完成前保持请求阶段命令卡片的语义标题。
  const displayCommand = hydratedPresentation.command || t(
    hydratedPresentation.commandSessionKind === "terminal"
      ? "toolPresentation.command.terminalTitle"
      : "toolPresentation.command.shellTitle",
  );
  // commandTitle exposes both the original command and generic contribution details without plugin-specific fields.
  // commandTitle 在不引入插件专用字段的前提下展示原始命令与通用贡献详情。
  const commandTitle = [
    displayCommand,
    ...pluginContributions.map((contribution) => `${contribution.label}: ${contribution.value}`),
  ].join("\n");
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
        title={commandTitle}
        type="button"
      >
        <TerminalSquare className={cn("terminal-session-icon", terminalState && terminalStateClass(terminalState))} size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        {pluginContributions.map((contribution, index) => contribution.badge ? (
          <span
            className="tool-presentation-route-badge"
            key={`${contribution.badge}:${contribution.label}:${index}`}
            title={`${contribution.label}: ${contribution.value}`}
          >
            {contribution.badge}
          </span>
        ) : null)}
        <span className="tool-presentation-command"><span className="tool-presentation-command-text">{displayCommand}</span></span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`command-detail:${presentation.id}`} expanded={expanded}>
        <div className="tool-presentation-detail">
          <pre className="tool-presentation-command-line"><span>{detailHeading}</span> {detailCommandText}</pre>
          {hydratedPresentation.cwd || hydratedPresentation.shellLabel ? (
            <div className="tool-presentation-terminal-meta">
              {hydratedPresentation.shellLabel ? <span>{hydratedPresentation.shellLabel}</span> : null}
              {hydratedPresentation.cwd ? <span>{hydratedPresentation.cwd}</span> : null}
            </div>
          ) : null}
          <div className="tool-presentation-output-scroll">
            <TerminalOutputView
              className="tool-presentation-output-text"
              emptyText={hydratedPresentation.state === "running"
                ? t("toolPresentation.execution.running")
                : t("toolPresentation.output.empty")}
              text={outputText}
            />
          </div>
          {hydratedPresentation.failureReason ? (
            <div className="tool-presentation-terminal-error">{hydratedPresentation.failureReason}</div>
          ) : null}
          <div className="tool-presentation-footer">
            <span>{sessionMetaLabel}</span>
            {hydratedPresentation.exitCode !== null ? (
              <span className={cn("tool-presentation-exit-code", exitCodeState)}>{t("toolPresentation.exitCode")} {exitCodeLabel}</span>
            ) : null}
          </div>
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// commandSessionActionStatusLabel returns an action result label instead of the session lifecycle label.
// commandSessionActionStatusLabel 返回动作结果文案，而不是会话生命周期文案。
// commandDetailHeading names the expanded content by command-session action.
// commandDetailHeading 根据命令会话动作命名展开内容。
function commandDetailHeading(
  presentation: CommandExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  if (presentation.commandSessionAction === "start") return t("toolPresentation.command.heading.start");
  if (presentation.commandSessionAction === "read") return t("toolPresentation.command.heading.read");
  if (presentation.commandSessionAction === "send") return t("toolPresentation.command.heading.send");
  if (presentation.commandSessionAction === "write") return t("toolPresentation.command.heading.write");
  if (presentation.commandSessionAction === "stop") return t("toolPresentation.command.heading.stop");
  return "$";
}

// commandDetailText returns the primary text shown on the first expanded line.
// commandDetailText 返回展开区首行展示的主要文本。
function commandDetailText(presentation: CommandExecutionPresentation): string {
  if (presentation.commandSessionAction === "write" || presentation.commandSessionAction === "send") {
    return presentation.commandSessionInput?.trim() || presentation.command;
  }
  return presentation.effectiveCommand?.trim() || presentation.command;
}

// commandSessionMetaLabel builds the footer text without making old actions look failed.
// commandSessionMetaLabel 构建页脚文本，避免让历史动作看起来失败。
function commandSessionMetaLabel(
  presentation: CommandExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  if (!presentation.commandSessionId) {
    return "";
  }
  const id = presentation.commandSessionId.length > 8 ? presentation.commandSessionId.slice(0, 8) : presentation.commandSessionId;
  if (presentation.lost) {
    return `${t("toolPresentation.command.session")} ${id} · ${t("toolPresentation.command.lost")}`;
  }
  if (presentation.terminalState) {
    const currentState = formatTranslation(t, "toolPresentation.command.currentState", {
      state: localizedTerminalStateLabel(presentation.terminalState, t),
    });
    return `${t("toolPresentation.command.session")} ${id} · ${currentState}`;
  }
  return `${t("toolPresentation.command.session")} ${id}`;
}

// commandPresentationStatusLabel returns the localized command lifecycle label.
// commandPresentationStatusLabel 返回本地化后的命令生命周期文案。
// localizedTerminalStateLabel returns the localized terminal lifecycle label.
// localizedTerminalStateLabel 返回本地化后的终端生命周期文案。
function localizedTerminalStateLabel(
  state: TerminalVisualState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("terminalState.running");
  if (state === "waiting") return t("terminalState.waiting");
  if (state === "success") return t("terminalState.success");
  if (state === "failed") return t("terminalState.failed");
  if (state === "stopped") return t("terminalState.stopped");
  if (state === "lost") return t("terminalState.lost");
  return t("terminalState.unknown");
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
