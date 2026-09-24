import * as React from "react";
import { ConversationCollaborationStatus } from "./conversation-collaboration-card";
import { ConversationCollaborationStateContext } from "./conversation-collaboration-state";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { Button } from "@/components/ui/button";
import {
  Bot,
  Braces,
  ChevronDown,
  ChevronRight,
  Code2,
  FileText,
  FolderTree,
  FolderSearch,
  GitBranch,
  Hammer,
  ListChecks,
  MessageCircleQuestionMark,
  Pencil,
  Search,
  Target,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationToolResultTabs } from "./conversation-tool-result-tabs";
import { ConversationTodoResult } from "./conversation-todo-result";
import { ConversationMediaGenerationResult, isMediaGenerationTool } from "./conversation-media-generation-result";
import { ModelExtensionToolPayloadCards } from "@/components/conversation/conversation-model-extension-tool-payloads";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import { formatTurnStatsNumber, TurnStatsMetricRow } from "@/components/conversation/conversation-turn-stats-card";
import {
  terminalStateClass,
  type TerminalVisualState,
} from "@/components/conversation/conversation-terminal-model";
import {
  hydrateAskQuestionPresentation,
  hydrateCommandExecutionPresentation,
  hydrateFileAccessPresentation,
  hydrateLuaExecutionPresentation,
  hydrateMarkdownToolPresentation,
  hydrateSubagentOrchestrationPresentation,
  type AskQuestionPresentation,
  type AskQuestionPresentationItem,
  type CommandExecutionPresentation,
  type FileAccessItemPresentation,
  type FileAccessLineRangePresentation,
  type FileAccessPresentation,
  type FileChangeFilePresentation,
  type FileChangeOperation,
  type FileChangePresentation,
  type GenericToolMarkdownPresentation,
  type LuaExecutionPresentation,
  type MarkdownToolPresentation,
  type MarkdownToolPresentationIcon,
  type MarkdownToolSummaryTarget,
  type NativeSemanticToolPresentation,
  type ProviderToolExtractPresentation,
  type ProviderToolSearchPresentation,
  type ProviderToolStructuredPresentation,
  type ProviderToolTextPresentation,
  type SubagentOrchestrationAgent,
  type SubagentOrchestrationPresentation,
  type ToolPresentation,
  type ToolPresentationDisplayState,
} from "@/components/conversation/conversation-tool-presentations";
import {
  modelExtensionToolCapability,
  modelExtensionToolMarkdown,
} from "@/components/conversation/tool-presentations/model-extension-tool-presentation";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";

// The exact event is shared with expanded bodies without rebuilding payloads from summaries.
// 精确事件与展开正文共享，不从摘要重建载荷。
const ToolResultEventContext = React.createContext<ToolCallEvent | undefined>(undefined);

/** Wraps result-oriented bodies with canonical tabs while preserving dedicated controls and source cards.
 * 为结果型正文包裹规范标签，同时保留专属控制与源码卡。
 * Parameter children is the original specialized renderer; returns that renderer or its tabbed shell.
 * 参数 children 为原专属渲染器；返回该渲染器或其标签外壳。 */
function ToolResultBody(props: { children: React.ReactNode }) {
  // event is absent only on the supported legacy rendering path.
  // event 仅在受支持的旧版渲染路径缺失。
  const event = React.useContext(ToolResultEventContext);
  return event ? <ConversationToolResultTabs event={event}>{props.children}</ConversationToolResultTabs> : <>{props.children}</>;
}

// ToolRequestCardProgress carries request-state text without replacing the resolved presentation model.
// ToolRequestCardProgress 在不替换已解析展示模型的前提下携带请求状态文案。
interface ToolRequestCardProgress {
  // Whether the card is still in the request phase and may use a stable activity treatment.
  // 卡片是否仍处于请求阶段并可使用稳定的活动态样式。
  receiving: boolean;
  // Request-receiving state that overrides premature execution wording.
  // 覆盖过早执行文案的请求接收状态。
  statusLabel: string;
}

// ToolDetailLoader resolves one authoritative hidden detail record after tool completion.
// ToolDetailLoader 在工具完成后解析单条权威隐藏详情记录。
type ToolDetailLoader = (
  detailRef: ConversationDetailRef,
) => Promise<ToolDetailPayload | null>;

/** Renders the original card and exposes failures from its existing read-only detail loader.
 * 渲染原卡片，并公开既有只读详情加载器的失败。
 * props retains the exact tool identity, expansion policy, and authoritative loader.
 * props 保留精确工具身份、展开策略和权威加载器。
 * Returns the card plus a retryable diagnostic without changing tool execution state.
 * 返回卡片和可重读诊断，不改变工具执行状态。 */
export function ToolPresentationCard(props: React.ComponentProps<typeof ToolPresentationCardContent>) {
  // t localizes only the diagnostic controls, preserving original service errors verbatim.
  // t 仅本地化诊断控件，原样保留服务错误。
  const { t } = useI18n();
  // failure belongs to this exact presentation rather than the last card rendered here.
  // failure 属于当前精确展示，而不是此位置之前渲染的卡片。
  const [failure, setFailure] = React.useState<{ owner: string; message: string } | null>(null);
  // retryRevision invalidates the existing hydration effect without issuing a new tool call.
  // retryRevision 使既有水合 effect 失效，不发起新的工具调用。
  const [retryRevision, setRetryRevision] = React.useState(0);
  // owner tracks the currently displayed identity when an older read completes asynchronously.
  // owner 在旧读取异步结束时跟踪当前展示身份。
  const owner = React.useRef(props.presentation.id);
  owner.current = props.presentation.id;
  /** Reads the same authoritative detail reference and preserves rejection for existing hydration logic.
   * 读取相同权威详情引用，并保留拒绝供既有水合逻辑处理。 */
  const loadDetail = React.useCallback<ToolDetailLoader>(async (detailRef) => {
    try {
      // payload remains nullable exactly as declared by the existing detail-loader contract.
      // payload 完全按照既有详情加载合同保留可空性。
      const payload = await props.onLoadToolDetail(detailRef);
      if (owner.current === props.presentation.id) setFailure(null);
      return payload;
    } catch (error) {
      if (owner.current === props.presentation.id) {
        setFailure({ owner: props.presentation.id, message: error instanceof Error ? error.message : String(error) });
      }
      throw error;
    }
  }, [props.onLoadToolDetail, props.presentation.id, retryRevision]);
  return (
    <>
      <ToolResultEventContext.Provider value={props.v1ToolEvent}>
        <ToolPresentationCardContent {...props} onLoadToolDetail={loadDetail} />
      </ToolResultEventContext.Provider>
      {failure?.owner === props.presentation.id ? (
        <div className="markdown-tool-block" role="alert">
          <details>
            <summary>{t("conversation.control.detailFailed")}</summary>
            <pre>{failure.message}</pre>
          </details>
          <Button size="sm" variant="outline" onClick={() => {
            setFailure(null);
            if (!props.requestProgress) props.onExpandedChange(props.presentation.id, true);
            setRetryRevision((revision) => revision + 1);
          }} type="button">{t("conversation.control.retry")}</Button>
        </div>
      ) : null}
    </>
  );
}

// ToolPresentationCardContent renders one recognized tool presentation with shared expansion behavior.
// ToolPresentationCardContent 使用共享展开行为渲染单个已识别工具展示。
function ToolPresentationCardContent(props: {
  // Exact canonical event retained by the branch projection; absent only for legacy transcripts.
  // 分支投影保留的精确规范事件；仅旧版转录缺失。
  v1ToolEvent?: ToolCallEvent;
  // Presentation model resolved by the adapter layer.
  // 由适配器层解析出的展示模型。
  presentation: ToolPresentation;
  // Whether the card is expanded in the current transcript surface.
  // 当前转录界面中该卡片是否已展开。
  expanded: boolean;
  // Optional non-native raw-detail action rendered only inside expanded content.
  // 仅在展开内容内部渲染的可选非原生原始详情操作。
  expandedFooter?: React.ReactNode;
  // Optional request stream state rendered by the resolved original card.
  // 由已解析原始卡片渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Whether the authoritative terminal detail reference is eligible for hydration.
  // 权威终态详情引用是否具备水合资格。
  detailLoadEnabled: boolean;
  // Expansion id set shared with the native-flow transcript surface.
  // 与原生流转录界面共享的展开标识集合。
  expandedToolPresentationIds: Set<string>;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for rich presentation data.
  // 加载单条隐藏工具详情记录用于补充展示数据。
  onLoadToolDetail: ToolDetailLoader;
}) {
  // expanded is forcibly closed while the model is still forming the request.
  // expanded 在模型仍在构造请求时被强制关闭。
  const expanded = props.requestProgress ? false : props.expanded;
  // handleExpandedChange rejects request-phase expansion without altering persisted state.
  // handleExpandedChange 拒绝请求阶段的展开操作，且不改变持久化状态。
  const handleExpandedChange = React.useCallback((presentationId: string, nextExpanded: boolean): void => {
    if (!props.requestProgress) {
      props.onExpandedChange(presentationId, nextExpanded);
    }
  }, [props.onExpandedChange, props.requestProgress]);
  if (props.presentation.kind === "command_execution") {
    return (
      <CommandExecutionPresentationCard
        expanded={expanded}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "file_change") {
    return (
      <FileChangePresentationCard
        expanded={expanded}
        expandedToolPresentationIds={props.expandedToolPresentationIds}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
      />
    );
  }
  if (
    props.presentation.kind === "file_read"
    || props.presentation.kind === "file_list"
    || props.presentation.kind === "file_search"
    || props.presentation.kind === "file_stat"
  ) {
    return (
      <FileAccessPresentationCard
        expanded={expanded}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "markdown_tool_output") {
    return (
      <MarkdownToolPresentationCard
        v1ToolEvent={props.v1ToolEvent}
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "lua_execution") {
    return (
      <LuaExecutionPresentationCard
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "ask_question") {
    return (
      <AskQuestionPresentationCard
        expanded={expanded}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "subagent_orchestration") {
    return (
      <SubagentOrchestrationPresentationCard
        expanded={expanded}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "provider_tool_search") {
    return (
      <ProviderToolSearchPresentationCard
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
      />
    );
  }
  if (props.presentation.kind === "provider_tool_text") {
    return (
      <ProviderToolTextPresentationCard
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
      />
    );
  }
  if (props.presentation.kind === "provider_tool_extract") {
    return (
      <ProviderToolExtractPresentationCard
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
      />
    );
  }
  if (props.presentation.kind === "provider_tool_structured") {
    return (
      <ProviderToolStructuredPresentationCard
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
      />
    );
  }
  if (props.presentation.kind === "native_semantic") {
    return (
      <NativeSemanticToolPresentationCard
        v1ToolEvent={props.v1ToolEvent}
        expanded={expanded}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  if (props.presentation.kind === "generic_tool_markdown") {
    if (props.presentation.titleKey) {
      return (
        <ModelExtensionToolPresentationCard
          v1ToolEvent={props.v1ToolEvent}
          expanded={props.expanded}
          expandedFooter={props.expandedFooter}
          presentation={props.presentation}
          requestProgress={props.requestProgress}
          onExpandedChange={props.onExpandedChange}
          onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
        />
      );
    }
    return (
      <GenericToolMarkdownPresentationCard
        v1ToolEvent={props.v1ToolEvent}
        expanded={expanded}
        expandedFooter={props.expandedFooter}
        presentation={props.presentation}
        requestProgress={props.requestProgress}
        onExpandedChange={handleExpandedChange}
        onLoadToolDetail={props.detailLoadEnabled ? props.onLoadToolDetail : undefined}
      />
    );
  }
  return null;
}

// ModelExtensionToolPresentationCard renders one extension call with independent input and output cards.
// ModelExtensionToolPresentationCard 使用相互独立的输入卡与返回卡渲染一次扩展调用。
// Parameters: props carries the localized extension model, active state and optional result footer.
// 参数：props 携带本地化扩展模型、活动状态及可选返回页脚。
// Returns: a stable tool header followed by the available independently folded payload cards.
// 返回值：稳定工具标题，以及当前可用且可独立折叠的载荷卡片。
function ModelExtensionToolPresentationCard(props: {
  // Canonical event supplies the same domain projection as the direct V1 renderer.
  // 规范事件提供与直接 V1 渲染器相同的领域投影。
  v1ToolEvent?: ToolCallEvent;
  // Whether the main extension tool card is expanded.
  // 扩展工具主卡是否展开。
  expanded: boolean;
  // Optional result-owned raw invocation action.
  // 可选的返回侧原始调用操作。
  expandedFooter?: React.ReactNode;
  // Recognized extension presentation with a localized title.
  // 具有本地化标题的已识别扩展展示。
  presentation: GenericToolMarkdownPresentation;
  // Optional request-formation state shown without disabling input expansion.
  // 可选的请求构造状态，且不会禁用输入展开。
  requestProgress: ToolRequestCardProgress | null;
  // Optional terminal result loader used to retain complete output.
  // 用于保留完整返回的可选终态结果加载器。
  onLoadToolDetail?: ToolDetailLoader;
  // Updates the main extension tool card expansion state.
  // 更新扩展工具主卡展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  // t resolves the extension title and active execution label.
  // t 解析扩展标题与活动执行标签。
  const { t } = useI18n();
  // titleKey is guaranteed by the exact dispatch branch and remains explicitly checked here.
  // titleKey 由精确分派分支保证，并在此继续显式检查。
  const titleKey = props.presentation.titleKey;
  // hydratedBody preserves the complete terminal result when the visible transcript was truncated.
  // hydratedBody 在可见转录被截断时保留完整终态返回。
  const hydratedBody = useTerminalToolCardBody(
    props.presentation.detailRef,
    props.onLoadToolDetail,
  );
  if (!titleKey) {
    return null;
  }
  // status remains visible while the request is forming or the provider tool is executing.
  // status 在请求构造或供应商工具执行期间保持可见。
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  // outputMarkdown formats the exact visible result independently from the request payload.
  // outputMarkdown 独立于请求载荷格式化精确可见返回。
  const outputMarkdown = modelExtensionToolMarkdown(
    props.presentation.toolName,
    hydratedBody ?? props.presentation.markdown,
  );
  // capability selects the same family icon used by the capability settings panel.
  // capability 选择与能力设置面板相同的能力族图标。
  const capability = modelExtensionToolCapability(props.presentation.toolName);
  // handleToggle updates only the main card and keeps payload expansion memories independent.
  // handleToggle 仅更新主卡，并保持载荷展开记忆相互独立。
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [props]);
  return (
    <div
      className={cn("tool-presentation-card", "model-extension-tool", props.expanded && "expanded")}
      data-tool-state={props.presentation.state}
    >
      <button
        aria-expanded={props.expanded}
        className="tool-presentation-summary model-extension-tool-header"
        onClick={handleToggle}
        type="button"
      >
        <span
          className="v1-tool-card-icon markdown-tool-icon"
          aria-hidden="true"
        >
          {capability
            ? <ProviderToolCapabilityIcon capability={capability} size={15} />
            : <Code2 size={15} />}
        </span>
        {status ? (
          <span className={cn("tool-presentation-state", "shimmer")}>{status}</span>
        ) : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{t(titleKey)}</span>
        </span>
        {props.expanded
          ? <ChevronDown className="tool-presentation-toggle" size={15} />
          : <ChevronRight className="tool-presentation-toggle" size={15} />}
      </button>
      <ConversationMeasuredCollapse
        collapseId={`model-extension-tool:${props.presentation.id}`}
        expanded={props.expanded}
      >
        {props.v1ToolEvent ? <ConversationToolResultTabs event={props.v1ToolEvent}>
          {isMediaGenerationTool(props.v1ToolEvent.tool_name)
            ? <ConversationMediaGenerationResult event={props.v1ToolEvent} />
            : <BranchToolDomainResult event={props.v1ToolEvent} id={props.presentation.id} />}
          {props.expandedFooter}
        </ConversationToolResultTabs> : <ModelExtensionToolPayloadCards
          inputMarkdown={props.presentation.inputMarkdown ?? ""}
          outputMarkdown={outputMarkdown}
          outputSupplement={props.expandedFooter}
          presentationId={props.presentation.id}
        />}
      </ConversationMeasuredCollapse>
    </div>
  );
}

// NativeSemanticToolPresentationCard renders known native tools without exposing raw tool ids.
// NativeSemanticToolPresentationCard 渲染已知原生工具，且不暴露原始工具标识。
function NativeSemanticToolPresentationCard(props: {
  // Canonical event supplies declared native fields without parsing Markdown or guessing result paths.
  // 规范事件提供原生声明字段，无需解析 Markdown 或猜测结果路径。
  v1ToolEvent?: ToolCallEvent;
  // Whether the terminal semantic body is expanded.
  // 终态语义正文是否展开。
  expanded: boolean;
  // Native semantic presentation model.
  // 原生语义展示模型。
  presentation: NativeSemanticToolPresentation;
  // Optional request-phase state.
  // 可选请求阶段状态。
  requestProgress: ToolRequestCardProgress | null;
  // Optional terminal detail loader.
  // 可选终态详情加载器。
  onLoadToolDetail?: ToolDetailLoader;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  // t resolves the semantic title and stable lifecycle labels.
  // t 解析语义标题与稳定生命周期文案。
  const { t } = useI18n();
  // hydratedBody contains terminal output only after the hidden detail is available.
  // hydratedBody 仅在隐藏详情可用后包含终态输出。
  const hydratedBody = useTerminalToolCardBody(props.presentation.detailRef, props.onLoadToolDetail);
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  // title is the localized operation name restored from the branch presentation contract.
  // title 是从分支展示契约恢复的本地化操作名称。
  const title = t(props.presentation.titleKey);
  // targetLabel exposes the exact prompt-skill name while the detail remains folded.
  // targetLabel 在详情折叠时展示精确的提示词技能名称。
  const targetLabel = nativeSemanticTargetLabel(props.presentation);
  // Icon follows the typed tool family and reuses the existing Goal, Plan and Skill symbols.
  // Icon 按强类型工具族复用既有目标、方案与技能图标。
  const Icon = props.presentation.family === "goal"
    ? Target
    : props.presentation.family === "todo"
      ? ListChecks
      : props.presentation.family === "skill"
        ? Hammer
        : Braces;
  // markdown keeps TODO previews authoritative while other families prefer terminal output.
  // markdown 保持 TODO 预览权威，其他卡片族优先使用终态输出。
  const markdown = props.presentation.family === "todo"
    ? nativeSemanticCardMarkdown(props.presentation)
    : hydratedBody ?? nativeSemanticCardMarkdown(props.presentation);
  // canExpand keeps every native semantic result inside the same explicit folded-card contract.
  // canExpand 让每个原生语义结果都遵循同一套显式折叠卡片契约。
  const canExpand = Boolean(markdown || props.v1ToolEvent);
  // handleToggle changes only the exact card expansion identity owned by the transcript surface.
  // handleToggle 只改变转录界面持有的精确卡片展开身份。
  const handleToggle = React.useCallback(() => {
    if (!canExpand || props.requestProgress) return;
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [canExpand, props]);
  return (
    <div className={cn("tool-presentation-card markdown-tool native-semantic-tool", canExpand && props.expanded && "expanded")}>
      <button
        aria-disabled={!canExpand || Boolean(props.requestProgress)}
        aria-expanded={canExpand && props.expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        title={targetLabel ? `${title} · ${targetLabel}` : title}
        type="button"
      >
        <Icon className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{title}{targetLabel ? ` · ${targetLabel}` : ""}</span>
        </span>
        {canExpand ? (
          props.expanded
            ? <ChevronDown className="tool-presentation-toggle" size={15} />
            : <ChevronRight className="tool-presentation-toggle" size={15} />
        ) : null}
      </button>
      <ConversationMeasuredCollapse collapseId={`native-semantic:${props.presentation.id}`} expanded={canExpand && props.expanded}>
        <ToolResultBody>
        {props.v1ToolEvent?.tool_name === "todolist" ? <ConversationTodoResult event={props.v1ToolEvent} /> : props.expanded && props.v1ToolEvent ? <BranchToolDomainResult event={props.v1ToolEvent} id={props.presentation.id} /> : canExpand && markdown ? (
          <div className="markdown-tool-block native-semantic-tool-body">
            <MarkdownMessage content={markdown} eager />
          </div>
        ) : null}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// nativeSemanticTargetLabel returns the exact collapsed-summary target for supported semantic tools.
// nativeSemanticTargetLabel 返回受支持语义工具在折叠摘要中的精确目标。
function nativeSemanticTargetLabel(presentation: NativeSemanticToolPresentation): string {
  if (presentation.toolId !== "skill") return "";
  const argumentsValue = presentation.arguments;
  if (!argumentsValue || typeof argumentsValue !== "object" || Array.isArray(argumentsValue)) return "";
  const name = (argumentsValue as Record<string, unknown>).name;
  return typeof name === "string" ? name.trim() : "";
}

// GenericToolMarkdownPresentationCard renders unknown tool output inline as Markdown.
// GenericToolMarkdownPresentationCard 将未知工具输出以内联 Markdown 形式渲染。
function GenericToolMarkdownPresentationCard(props: {
  // Canonical event keeps unknown tool data behind the same bounded domain and raw-detail view.
  // 规范事件让未知工具数据使用相同的有界领域与原始详情视图。
  v1ToolEvent?: ToolCallEvent;
  // Whether raw details are expanded.
  // 原始详情是否展开。
  expanded: boolean;
  // Optional raw-detail action owned by the expanded card.
  // 展开卡片持有的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Unknown-tool fallback presentation.
  // 未知工具兜底展示模型。
  presentation: GenericToolMarkdownPresentation;
  // Optional request-phase state.
  // 可选请求阶段状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Optional terminal detail loader.
  // 可选终态详情加载器。
  onLoadToolDetail?: ToolDetailLoader;
}) {
  // t resolves stable lifecycle labels for the unknown-tool card.
  // t 为未知工具卡解析稳定生命周期文案。
  const { t } = useI18n();
  // hydratedBody contains the complete terminal body when hidden detail loading succeeds.
  // hydratedBody 在隐藏详情加载成功时包含完整终态正文。
  const hydratedBody = useTerminalToolCardBody(props.presentation.detailRef, props.onLoadToolDetail);
  // status is visible only while request formation or real execution is active.
  // status 仅在请求构造或真实执行仍进行时显示。
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  // markdown uses terminal detail when available and otherwise keeps the visible result.
  // markdown 在终态详情可用时使用详情，否则保留可见结果。
  const markdown = modelExtensionToolMarkdown(
    props.presentation.toolName,
    hydratedBody ?? props.presentation.markdown,
  );
  // canExpand keeps every unknown tool payload inside a bounded folded card even without a legacy raw-detail RPC.
  // canExpand 即使没有旧 raw-detail RPC，也会把每个未知工具载荷放入有界折叠卡片。
  const canExpand = Boolean(markdown || props.expandedFooter || props.v1ToolEvent);
  // handleToggle opens only the expanded card content and never acts during request formation.
  // handleToggle 仅打开卡片展开内容，且绝不在请求构造阶段生效。
  const handleToggle = React.useCallback(() => {
    if (!canExpand) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [canExpand, props]);
  return (
    <div className={cn("tool-presentation-card markdown-tool generic-tool-markdown", props.expanded && "expanded")}>
      <button
        aria-disabled={!canExpand}
        aria-expanded={canExpand && props.expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        type="button"
      >
        <Code2 className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">
            {props.presentation.titleKey ? t(props.presentation.titleKey) : props.presentation.toolName}
          </span>
        </span>
        {canExpand ? (
          props.expanded
            ? <ChevronDown className="tool-presentation-toggle" size={15} />
            : <ChevronRight className="tool-presentation-toggle" size={15} />
        ) : null}
      </button>
      <ConversationMeasuredCollapse collapseId={`generic-tool:${props.presentation.id}`} expanded={canExpand && props.expanded}>
        {props.expanded && props.v1ToolEvent ? <BranchToolDomainResult event={props.v1ToolEvent} id={props.presentation.id} tabbed /> : markdown ? (
          <div className="markdown-tool-block generic-tool-markdown-body">
            <MarkdownMessage content={markdown} eager />
          </div>
        ) : null}
        {props.expandedFooter}
      </ConversationMeasuredCollapse>
    </div>
  );
}

/** Shares exact result projection between the live branch surface and the direct V1 renderer.
 * 在实际分支界面与直接 V1 渲染器之间共享精确结果投影。
 * Parameters event and id identify payload ownership; tabbed selects peer result/input/output tabs. Returns readable tool details.
 * 参数 event 与 id 标识载荷归属；tabbed 选择平级返回信息、输入、输出标签页。返回可读工具详情。 */
function BranchToolDomainResult(props: { event: ToolCallEvent; id: string; tabbed?: boolean }) {
  // Translation belongs to the stable card shell, including failures before the deferred module loads.
  // 翻译由稳定卡片外壳提供，包括延迟模块尚未加载时的失败。
  const { t } = useI18n();
  // An explicit retry creates a fresh lazy instance instead of reusing React's cached rejection.
  // 明确重试时创建新的延迟实例，避免复用 React 缓存的失败。
  const [attempt, setAttempt] = React.useState(0);
  // Domain rendering and projection are loaded only when this expanded result mounts.
  // 仅在展开结果挂载时加载领域展示及投影。
  const LazyDomain = React.useMemo(() => React.lazy(() => import("./conversation-branch-tool-domain")), [attempt]);
  return <RenderErrorBoundary key={attempt} fallback={(error) => <div role="alert" className="grid gap-2">
    <span>{t("toolPresentation.output.loadFailed")}</span>
    <Button size="sm" variant="outline" onClick={() => setAttempt((value) => value + 1)}>{t("toolPresentation.output.retryLoad")}</Button>
    <details><summary>{t("toolPresentation.output.errorDetails")}</summary><pre className="whitespace-pre-wrap break-words">{error.message}</pre></details>
  </div>}>
    <React.Suspense fallback={<StreamPlaceholder className="py-1" label={t("toolPresentation.output.loading")} count={3} />}>
      <LazyDomain event={props.event} id={props.id} tabbed={props.tabbed} />
    </React.Suspense>
  </RenderErrorBoundary>;
}

// activeToolStatusLabel keeps transient progress visible and removes generic terminal success or failure labels.
// activeToolStatusLabel 保留瞬时进度，并移除通用终态成功或失败文案。
// Parameters: requestProgress owns request formation, state owns execution, and runningLabel is card-specific copy.
// 参数：requestProgress 持有请求构造，state 持有执行状态，runningLabel 是卡片专用文案。
// Returns: a transient status label, or null after the tool reaches any terminal state.
// 返回值：返回瞬时状态文案；工具进入任意终态后返回 null。
function activeToolStatusLabel(
  requestProgress: ToolRequestCardProgress | null,
  state: ToolPresentationDisplayState,
  runningLabel: string,
): string | null {
  if (requestProgress) {
    return requestProgress.statusLabel;
  }
  return state === "running" ? runningLabel : null;
}

// useTerminalToolCardBody hydrates terminal output without projecting loading failures into lifecycle state.
// useTerminalToolCardBody 水合终态输出，且不会把加载失败投影到生命周期状态。
function useTerminalToolCardBody(
  detailRef: ConversationDetailRef | undefined,
  onLoadToolDetail: ToolDetailLoader | undefined,
): string | null {
  // body stores only a successfully loaded non-empty terminal result.
  // body 仅保存成功加载的非空终态结果。
  const [body, setBody] = React.useState<string | null>(null);
  React.useEffect(() => {
    setBody(null);
    if (!detailRef || !onLoadToolDetail) {
      return;
    }
    // cancelled prevents a stale detail request from updating an unmounted or rebound card.
    // cancelled 防止过期详情请求更新已卸载或已重新绑定的卡片。
    let cancelled = false;
    void onLoadToolDetail(detailRef)
      .then((payload) => {
        if (!cancelled && payload?.body.trim()) {
          setBody(payload.body);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [detailRef, onLoadToolDetail]);
  return body;
}

// nativeSemanticCardMarkdown returns a compact business result and a dedicated TODO checklist preview.
// nativeSemanticCardMarkdown 返回紧凑业务结果，并为 TODO 提供专用检查清单预览。
function nativeSemanticCardMarkdown(presentation: NativeSemanticToolPresentation): string {
  if (presentation.family === "todo") {
    return todoPresentationMarkdown(presentation.result, presentation.arguments);
  }
  if (typeof presentation.result === "string" && presentation.result.trim()) {
    return presentation.result;
  }
  if (presentation.result && typeof presentation.result === "object") {
    return `\`\`\`json\n${JSON.stringify(presentation.result, null, 2)}\n\`\`\``;
  }
  return "";
}

// todoPresentationMarkdown renders authoritative TODO payloads or the current safe request preview.
// todoPresentationMarkdown 渲染权威 TODO 载荷或当前安全请求预览。
function todoPresentationMarkdown(result: unknown, argumentsValue: unknown): string {
  // candidate prefers the authoritative host result and falls back to the safe request preview.
  // candidate 优先使用权威宿主结果，并回退到安全请求预览。
  const candidate = result && typeof result === "object" && !Array.isArray(result)
    ? result as Record<string, unknown>
    : argumentsValue && typeof argumentsValue === "object" && !Array.isArray(argumentsValue)
      ? argumentsValue as Record<string, unknown>
      : null;
  // items is the exact TODO item array retained by the selected payload.
  // items 是选定载荷保留的精确 TODO 项数组。
  const items = candidate && Array.isArray(candidate.items) ? candidate.items : [];
  // rows converts only valid TODO items into compact Markdown checklist rows.
  // rows 仅把有效 TODO 项转换为紧凑 Markdown 清单行。
  const rows = items.flatMap((item) => {
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      return [];
    }
    // record exposes the fixed TODO item fields after object validation.
    // record 在对象校验后暴露固定 TODO 项字段。
    const record = item as Record<string, unknown>;
    // step is the non-empty user-facing task text.
    // step 是非空的面向用户任务文本。
    const step = typeof record.step === "string" ? record.step.trim() : "";
    if (!step) {
      return [];
    }
    // status maps the fixed TODO lifecycle to Markdown checklist syntax.
    // status 将固定 TODO 生命周期映射为 Markdown 清单语法。
    const status = record.status === "completed" ? "x" : record.status === "in_progress" ? "-" : " ";
    return [`- [${status}] ${step}`];
  });
  // explanation is the optional host-provided TODO update summary.
  // explanation 是宿主可选提供的 TODO 更新说明。
  const explanation = candidate && typeof candidate.explanation === "string" ? candidate.explanation.trim() : "";
  return [explanation, ...rows].filter(Boolean).join("\n");
}

// ProviderToolStructuredPresentationCard renders unified embedding, rerank, and composite output.
// ProviderToolStructuredPresentationCard 渲染统一向量、重排与复合输出。
function ProviderToolStructuredPresentationCard(props: {
  // Whether the card is expanded.
  // 卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Normalized structured provider-tool presentation.
  // 归一化结构化供应商工具展示。
  presentation: ProviderToolStructuredPresentation;
  // Optional request stream state.
  // 可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface expansion state.
  // 更新界面展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const { t } = useI18n();
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [props]);
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  const titleKey = providerToolStructuredTitleKey(props.presentation.outputKind);
  return (
    <div className={cn("tool-presentation-card markdown-tool", props.expanded && "expanded")}>
      <button
        aria-expanded={props.expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        type="button"
      >
        <Braces className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{t(titleKey)}</span>
        </span>
        <span className="text-muted-foreground text-xs">
          {props.presentation.itemCount} {t("toolPresentation.providerStructured.items")}
        </span>
        {props.expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`provider-structured:${props.presentation.id}`} expanded={props.expanded}>
        <ToolResultBody>
        <div className="min-w-0 px-3 pb-3">
          <pre className="max-h-80 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-md border bg-muted/30 p-2 text-xs">
            {JSON.stringify(props.presentation.payload, null, 2)}
          </pre>
        </div>
        {props.expandedFooter}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// providerToolStructuredTitleKey maps one closed output kind to its exact localized title.
// providerToolStructuredTitleKey 将一个封闭输出类型映射为精确本地化标题。
function providerToolStructuredTitleKey(
  outputKind: ProviderToolStructuredPresentation["outputKind"],
): TranslationKey {
  switch (outputKind) {
    case "embedding":
      return "toolPresentation.providerStructured.embedding";
    case "rerank":
      return "toolPresentation.providerStructured.rerank";
    case "composite":
      return "toolPresentation.providerStructured.composite";
  }
}

// ProviderToolTextPresentationCard renders unified provider-generated text.
// ProviderToolTextPresentationCard 渲染统一的供应商生成文本。
function ProviderToolTextPresentationCard(props: {
  // Whether the card is expanded.
  // 卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Normalized provider text presentation.
  // 归一化供应商文本展示。
  presentation: ProviderToolTextPresentation;
  // Optional request stream state.
  // 可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface expansion state.
  // 更新界面展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const { t } = useI18n();
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [props]);
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  return (
    <div className={cn("tool-presentation-card markdown-tool provider-search-tool", props.expanded && "expanded")}>
      <button
        aria-expanded={props.expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        type="button"
      >
        <FileText className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">
            {t("toolPresentation.providerText.title")}
          </span>
        </span>
        {props.expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`provider-text:${props.presentation.id}`} expanded={props.expanded}>
        <ToolResultBody>
        <div className="markdown-tool-block">
          {props.presentation.text.trim() ? (
            <MarkdownMessage content={props.presentation.text} eager />
          ) : (
            <div className="markdown-tool-empty">{t("toolPresentation.providerText.empty")}</div>
          )}
        </div>
        {props.expandedFooter}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// ProviderToolExtractPresentationCard renders unified provider extraction output.
// ProviderToolExtractPresentationCard 渲染统一供应商网页提取输出。
function ProviderToolExtractPresentationCard(props: {
  // Whether the card is expanded.
  // 卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Normalized provider extraction presentation.
  // 归一化供应商网页提取展示。
  presentation: ProviderToolExtractPresentation;
  // Optional request stream state.
  // 可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface expansion state.
  // 更新界面展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const { t } = useI18n();
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [props]);
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  return (
    <div className={cn("tool-presentation-card markdown-tool", props.expanded && "expanded")}>
      <button
        aria-expanded={props.expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        type="button"
      >
        <FileText className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">
            {t("toolPresentation.providerExtract.title")}
          </span>
        </span>
        <span className="text-muted-foreground text-xs">
          {props.presentation.results.length} {t("toolPresentation.providerExtract.results")}
          {props.presentation.failures.length ? ` · ${props.presentation.failures.length} ${t("toolPresentation.providerExtract.failures")}` : ""}
        </span>
        {props.expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`provider-extract:${props.presentation.id}`} expanded={props.expanded}>
        <ToolResultBody>
        <div className="grid gap-2 px-3 pb-3">
          {props.presentation.results.map((result) => (
            <article className="rounded-md border p-2 text-sm" key={result.url}>
              <a className="font-medium hover:underline" href={result.url} rel="noreferrer" target="_blank">
                {result.url}
              </a>
              <div className="mt-2 max-h-64 overflow-auto rounded bg-muted/40 p-2">
                <MarkdownMessage content={result.content} />
              </div>
              {result.images.length ? (
                <div className="text-muted-foreground mt-2 text-xs">
                  {result.images.length} {t("toolPresentation.providerExtract.images")}
                </div>
              ) : null}
            </article>
          ))}
          {props.presentation.failures.map((failure) => (
            <div className="rounded-md border border-destructive/30 p-2 text-sm" key={`failure:${failure.url}`}>
              <a className="font-medium hover:underline" href={failure.url} rel="noreferrer" target="_blank">
                {failure.url}
              </a>
              <p className="text-destructive mt-1">{failure.error}</p>
            </div>
          ))}
          {!props.presentation.results.length && !props.presentation.failures.length ? (
            <div className="markdown-tool-empty">{t("toolPresentation.providerExtract.empty")}</div>
          ) : null}
        </div>
        {props.expandedFooter}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// ProviderToolSearchPresentationCard renders the unified provider search output.
// ProviderToolSearchPresentationCard 渲染统一供应商搜索输出。
//
// Parameters:
// `presentation` is the normalized result and expansion props belong to the transcript surface.
// 参数：
// `presentation` 是归一化结果，展开属性属于转录界面。
//
// Returns:
// One shared-style expandable tool card independent of the upstream search provider.
// 返回值：
// 返回一张与上游搜索供应商无关的共享样式可展开工具卡片。
function ProviderToolSearchPresentationCard(props: {
  // Whether the card is expanded.
  // 卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Normalized provider search presentation.
  // 归一化供应商搜索展示。
  presentation: ProviderToolSearchPresentation;
  // Optional request stream state.
  // 可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface expansion state.
  // 更新界面展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
}) {
  const { t } = useI18n();
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    props.onExpandedChange(props.presentation.id, !props.expanded);
  }, [props]);
  const status = activeToolStatusLabel(
    props.requestProgress,
    props.presentation.state,
    t("toolPresentation.execution.running"),
  );
  return (
    <div className={cn("tool-presentation-card markdown-tool", props.expanded && "expanded")}>
      <button
        aria-expanded={props.expanded}
        className="tool-presentation-summary markdown-tool-summary provider-search-summary"
        onClick={handleToggle}
        title={props.presentation.query}
        type="button"
      >
        <Search className="markdown-tool-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">
            {t("toolPresentation.providerSearch.title")} · {props.presentation.query}
          </span>
        </span>
        <span className="provider-search-result-count text-muted-foreground text-xs">
          {props.presentation.results.length} {t("toolPresentation.providerSearch.results")}
        </span>
        {props.expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`provider-search:${props.presentation.id}`} expanded={props.expanded}>
        <ToolResultBody>
        <div className="grid min-w-0 max-w-full gap-2 overflow-hidden px-3 pb-3">
          {props.presentation.results.length ? props.presentation.results.map((result) => (
            <a
              className="min-w-0 max-w-full overflow-hidden rounded-md border p-2 text-sm transition-colors hover:bg-muted/50"
              href={result.url}
              key={`${result.rank}:${result.url}`}
              rel="noreferrer"
              target="_blank"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span className="text-muted-foreground text-xs">{result.rank}</span>
                <strong className="min-w-0 flex-1 truncate">{result.title}</strong>
                <span className="text-muted-foreground max-w-40 truncate text-xs">{result.sourceDomain}</span>
              </div>
              <p className="text-muted-foreground mt-1 line-clamp-2 [overflow-wrap:anywhere]">{result.snippet}</p>
            </a>
          )) : (
            <div className="markdown-tool-empty">{t("toolPresentation.providerSearch.empty")}</div>
          )}
        </div>
        {props.expandedFooter}
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// commandPresentationDetailCache avoids refetching hidden details after ordinary remounts.
// commandPresentationDetailCache 避免普通重新挂载后重复请求隐藏详情。
const commandPresentationDetailCache = new Map<string, CommandExecutionPresentation>();

// askQuestionPresentationDetailCache avoids refetching completed ask answers after ordinary remounts.
// askQuestionPresentationDetailCache 避免普通重新挂载后重复请求已完成 ask 答案。
const askQuestionPresentationDetailCache = new Map<string, AskQuestionPresentation>();

// markdownToolPresentationDetailCache avoids refetching full Markdown-output detail after remounts.
// markdownToolPresentationDetailCache 避免重挂后重复请求完整 Markdown 输出详情。
const markdownToolPresentationDetailCache = new Map<string, MarkdownToolPresentation>();

// luaExecutionPresentationDetailCache avoids refetching full RunLua results after ordinary remounts.
// luaExecutionPresentationDetailCache 避免普通重新挂载后重复请求完整 RunLua 结果。
const luaExecutionPresentationDetailCache = new Map<string, LuaExecutionPresentation>();

// fileAccessPresentationDetailCache avoids refetching native read-only results after ordinary remounts.
// fileAccessPresentationDetailCache 避免原生只读结果在普通重挂后重复请求。
const fileAccessPresentationDetailCache = new Map<string, FileAccessPresentation>();

// subagentOrchestrationPresentationDetailCache avoids refetching hidden subagent status details.
// subagentOrchestrationPresentationDetailCache 避免重复请求隐藏子代理状态详情。
const subagentOrchestrationPresentationDetailCache = new Map<string, SubagentOrchestrationPresentation>();

// MarkdownToolPresentationCard renders exact-tool Markdown output.
// MarkdownToolPresentationCard 渲染精确工具标识对应的 Markdown 输出。
function MarkdownToolPresentationCard(props: {
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

// LuaExecutionPresentationCard renders one-shot and persistent RunLua actions with Lua-specific detail.
// LuaExecutionPresentationCard 使用 Lua 专用详情渲染一次性与持久 RunLua 动作。
function LuaExecutionPresentationCard(props: {
  // Lua execution presentation model.
  // Lua 执行展示模型。
  presentation: LuaExecutionPresentation;
  // Whether this Lua execution card is expanded.
  // 当前 Lua 执行卡片是否展开。
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
  // Loads one hidden tool-detail record for the complete Lua result.
  // 加载单条隐藏工具详情记录用于完整 Lua 结果。
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
  const detailCacheKey = luaExecutionPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<LuaExecutionPresentation>(
    () => detailCacheKey ? luaExecutionPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? luaExecutionPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || luaExecutionPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateLuaExecutionPresentation(presentation, payload);
        luaExecutionPresentationDetailCache.set(detailCacheKey, nextPresentation);
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
  // title is the complete action title; only the Lua identity icon is prepended.
  // title 是完整动作标题；前方仅保留 Lua 身份图标。
  const title = luaExecutionActionLabel(hydratedPresentation, t);
  // status keeps request formation and actual execution visible without terminal prefixes.
  // status 在不添加终态前缀的前提下展示请求构造与真实执行状态。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  // argumentsMarkdown renders structured args as a two-column Markdown table.
  // argumentsMarkdown 将结构化参数渲染为两列表格 Markdown。
  const argumentsMarkdown = luaExecutionArgumentsMarkdown(hydratedPresentation.arguments, t);
  // outputMarkdown strips model-facing envelopes and keeps only user-relevant results.
  // outputMarkdown 移除面向模型的外层结构，只保留与用户相关的结果。
  const outputMarkdown = luaExecutionOutputMarkdown(hydratedPresentation, t);
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);
  return (
    <div className={cn("tool-presentation-card lua-execution", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary"
        onClick={handleToggle}
        title={title}
        type="button"
      >
        <LuaToolIcon className="lua-execution-icon" />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{title}</span>
        </span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`lua-execution:${presentation.id}`} expanded={expanded}>
        <ToolResultBody>
        <div className="tool-presentation-detail lua-execution-detail">
          {hydratedPresentation.code ? (
            <div className="lua-execution-section">
              <div className="lua-execution-section-title">{t("toolPresentation.lua.code")}</div>
              <MarkdownMessage content={markdownFencedCode("lua", hydratedPresentation.code)} eager />
            </div>
          ) : null}
          {argumentsMarkdown ? (
            <div className="lua-execution-section">
              <div className="lua-execution-section-title">{t("toolPresentation.lua.arguments")}</div>
              <MarkdownMessage content={argumentsMarkdown} eager />
            </div>
          ) : null}
          <div className="lua-execution-section lua-execution-output">
            <div className="lua-execution-section-title">{t("toolPresentation.lua.output")}</div>
            {outputMarkdown ? (
              <MarkdownMessage content={outputMarkdown} eager />
            ) : (
            <div className="markdown-tool-empty">{
              hydratedPresentation.state === "running"
                ? t("toolPresentation.execution.running")
                : t("toolPresentation.output.empty")
            }</div>
            )}
          </div>
          {props.expandedFooter}
        </div>
        </ToolResultBody>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// LuaToolIcon renders the compact Lua orbit mark without adding a status badge.
// LuaToolIcon 渲染紧凑的 Lua 轨道标志，不附加状态徽标。
function LuaToolIcon(props: {
  // Optional CSS class used by the conversation-card layout.
  // 会话卡片布局使用的可选 CSS 类名。
  className?: string;
}) {
  return (
    <svg
      aria-hidden="true"
      className={props.className}
      fill="none"
      focusable="false"
      viewBox="0 0 32 32"
    >
      <path
        d="M30 6a3.86 3.86 0 0 1-1.167 2.833 4.024 4.024 0 0 1-5.666 0A3.86 3.86 0 0 1 22 6a3.86 3.86 0 0 1 1.167-2.833 4.024 4.024 0 0 1 5.666 0A3.86 3.86 0 0 1 30 6m-9.208 5.208A10.6 10.6 0 0 0 13 8a10.6 10.6 0 0 0-7.792 3.208A10.6 10.6 0 0 0 2 19a10.6 10.6 0 0 0 3.208 7.792A10.6 10.6 0 0 0 13 30a10.6 10.6 0 0 0 7.792-3.208A10.6 10.6 0 0 0 24 19a10.6 10.6 0 0 0-3.208-7.792m-1.959 7.625a4.024 4.024 0 0 1-5.666 0 4.024 4.024 0 0 1 0-5.666 4.024 4.024 0 0 1 5.666 0 4.024 4.024 0 0 1 0 5.666"
        fill="currentColor"
      />
    </svg>
  );
}

// luaExecutionPresentationDetailCacheKey returns a stable cache key for one hidden Lua result.
// luaExecutionPresentationDetailCacheKey 返回单条隐藏 Lua 结果的稳定缓存键。
function luaExecutionPresentationDetailCacheKey(presentation: LuaExecutionPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return `${presentation.detailRef.recordId ?? ""}:${presentation.detailRef.seq ?? ""}:${presentation.detailRef.messageIndex ?? ""}`;
}

// luaExecutionActionLabel returns the complete localized public RunLua action title.
// luaExecutionActionLabel 返回完整的本地化公开 RunLua 动作标题。
function luaExecutionActionLabel(
  presentation: LuaExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  let actionLabel: string;
  if (presentation.action === "invalid") actionLabel = t("toolPresentation.lua.action.invalid");
  else if (presentation.action === "list") actionLabel = t("toolPresentation.lua.action.list");
  else if (presentation.action === "start") actionLabel = t("toolPresentation.lua.action.start");
  else if (presentation.action === "status") actionLabel = t("toolPresentation.lua.action.status");
  else if (presentation.action === "close") actionLabel = t("toolPresentation.lua.action.close");
  else actionLabel = t("toolPresentation.lua.action.eval");
  // Only persistent actions have a verified session name; one-shot task labels are not session identities.
  // 只有持久动作具备已验证的会话名称；一次性任务标签不是会话身份。
  return presentation.mode === "persistent" && presentation.label
    ? `${actionLabel} · ${presentation.label}`
    : actionLabel;
}

// LuaMarkdownCodeBlock preserves one exact fenced block extracted from LuaSkills output.
// LuaMarkdownCodeBlock 保留从 LuaSkills 输出中提取的一段精确围栏代码块。
interface LuaMarkdownCodeBlock {
  // Declared Markdown fence language.
  // Markdown 围栏声明的语言。
  language: string;
  // Exact fenced content without delimiters.
  // 不含分隔符的精确围栏内容。
  content: string;
}

// luaExecutionArgumentsMarkdown renders exact structured arguments as a compact Markdown table.
// luaExecutionArgumentsMarkdown 将精确结构化参数渲染为紧凑 Markdown 表格。
function luaExecutionArgumentsMarkdown(
  argumentsValue: Record<string, unknown> | null,
  t: (key: TranslationKey) => string,
): string {
  if (!argumentsValue || Object.keys(argumentsValue).length === 0) {
    return "";
  }
  const rows = Object.entries(argumentsValue).map(([name, value]) => (
    `| ${markdownTableCell(name)} | ${markdownTableCell(JSON.stringify(value))} |`
  ));
  return [
    `| ${t("toolPresentation.lua.argumentName")} | ${t("toolPresentation.lua.argumentValue")} |`,
    "| --- | --- |",
    ...rows,
  ].join("\n");
}

// luaExecutionOutputMarkdown converts model-facing RunLua envelopes into user-facing detail only.
// luaExecutionOutputMarkdown 将面向模型的 RunLua 外层结构转换为仅供用户查看的详情。
function luaExecutionOutputMarkdown(
  presentation: LuaExecutionPresentation,
  t: (key: TranslationKey) => string,
): string {
  const output = presentation.output.trim();
  if (!output) {
    return "";
  }
  return presentation.toolId === "runlua"
    ? luaOneShotOutputMarkdown(output)
    : luaSessionOutputMarkdown(presentation, output, t);
}

// luaOneShotOutputMarkdown extracts only returned values or the exact error detail from LuaSkills Markdown.
// luaOneShotOutputMarkdown 仅从 LuaSkills Markdown 中提取返回值或精确错误详情。
function luaOneShotOutputMarkdown(output: string): string {
  if (output.startsWith("# Runtime Execution Error")) {
    const errorSection = luaMarkdownSection(output, "Error");
    const errorDetail = errorSection ? luaMarkdownCodeBlocks(errorSection)[0]?.content : null;
    return markdownFencedCode("text", errorDetail ?? output);
  }
  if (!output.startsWith("# Runtime Execution Result")) {
    return markdownFencedCode("text", luaToolFailureDetail(output));
  }
  const returnedSection = luaMarkdownSection(output, "Returned Values") ?? "";
  const returnedValues = luaMarkdownCodeBlocks(returnedSection).map((block) => {
    if (block.language !== "json") {
      return block.content;
    }
    try {
      return JSON.parse(block.content) as unknown;
    } catch {
      return block.content;
    }
  });
  const printedSection = luaMarkdownSection(output, "Printed Output");
  const printedOutput = printedSection ? luaMarkdownCodeBlocks(printedSection)[0]?.content : null;
  const returnedValue = returnedValues.length === 1 ? returnedValues[0] : returnedValues;
  const displayValue = printedOutput
    ? { printed_output: printedOutput, returned_values: returnedValue }
    : returnedValue;
  return markdownFencedCode("json", luaJsonDisplayText(displayValue));
}

// luaSessionOutputMarkdown renders each persistent lifecycle action from its exact JSON response contract.
// luaSessionOutputMarkdown 根据精确 JSON 响应契约渲染各个持久会话生命周期动作。
function luaSessionOutputMarkdown(
  presentation: LuaExecutionPresentation,
  output: string,
  t: (key: TranslationKey) => string,
): string {
  const payload = luaJsonObject(output);
  if (!payload) {
    return markdownFencedCode("text", luaToolFailureDetail(output));
  }
  if (payload.ok !== true) {
    return markdownFencedCode("text", luaSessionErrorDetail(payload));
  }
  if (presentation.action === "eval") {
    if (!Object.hasOwn(payload, "result")) {
      return markdownFencedCode("text", "runlua_session eval response is missing `result`.");
    }
    return markdownFencedCode("json", luaJsonDisplayText(payload.result));
  }
  if (presentation.action === "list") {
    return luaSessionListMarkdown(payload, t);
  }
  if (presentation.action === "close") {
    const sessionId = typeof payload.lease_id === "string" ? payload.lease_id : "-";
    return t("toolPresentation.lua.closeSucceeded").replace("{session}", sessionId);
  }
  const displayValue = presentation.action === "start"
    ? {
        name: presentation.label,
        session_id: payload.lease_id,
        generation: payload.generation,
        ttl_sec: payload.ttl_sec,
        expires_at_unix_ms: payload.expires_at_unix_ms,
      }
    : {
        session_id: payload.lease_id,
        active: payload.ok,
        ttl_sec: payload.ttl_sec,
        expires_at_unix_ms: payload.expires_at_unix_ms,
        closed: payload.closed,
        expired: payload.expired,
      };
  return markdownFencedCode("json", luaJsonDisplayText(displayValue));
}

// luaSessionListMarkdown renders only the user-relevant session identity and lifecycle fields.
// luaSessionListMarkdown 仅渲染与用户相关的会话身份和生命周期字段。
function luaSessionListMarkdown(
  payload: Record<string, unknown>,
  t: (key: TranslationKey) => string,
): string {
  if (!Array.isArray(payload.sessions)) {
    return markdownFencedCode("text", "runlua_session list response is missing `sessions`.");
  }
  if (payload.sessions.length === 0) {
    return t("toolPresentation.lua.noSessions");
  }
  if (!payload.sessions.every((value) => value && typeof value === "object" && !Array.isArray(value))) {
    return markdownFencedCode("text", "runlua_session list response contains a non-object session.");
  }
  const rows = payload.sessions.map((sessionValue) => {
    const session = sessionValue as Record<string, unknown>;
    const ttl = typeof session.ttl_sec === "number" ? `${session.ttl_sec}s` : "-";
    const expiresAt = typeof session.expires_at_unix_ms === "number"
      ? new Date(session.expires_at_unix_ms).toLocaleString()
      : "-";
    return `| ${markdownTableCell(session.name)} | ${markdownTableCell(session.session_id)} | ${markdownTableCell(luaSessionStateLabel(session, t))} | ${ttl} | ${markdownTableCell(expiresAt)} |`;
  });
  return [
    `| ${t("toolPresentation.lua.sessionName")} | ${t("toolPresentation.lua.sessionId")} | ${t("toolPresentation.lua.sessionState")} | ${t("toolPresentation.lua.sessionTtl")} | ${t("toolPresentation.lua.sessionExpiresAt")} |`,
    "| --- | --- | --- | --- | --- |",
    ...rows,
  ].join("\n");
}

// luaSessionStateLabel maps exact lease flags to one localized display state.
// luaSessionStateLabel 将精确租约标志映射为一个本地化展示状态。
function luaSessionStateLabel(
  session: Record<string, unknown>,
  t: (key: TranslationKey) => string,
): string {
  if (session.ok === true) return t("toolPresentation.lua.sessionActive");
  if (session.expired === true) return t("toolPresentation.lua.sessionExpired");
  if (session.closed === true) return t("toolPresentation.lua.sessionClosed");
  return t("toolPresentation.lua.sessionFailed");
}

// luaSessionErrorDetail returns the exact runtime error code and message without lifecycle metadata.
// luaSessionErrorDetail 返回精确运行时错误代码与消息，不包含生命周期元数据。
function luaSessionErrorDetail(payload: Record<string, unknown>): string {
  const errorCode = typeof payload.error_code === "string" ? payload.error_code : null;
  const message = typeof payload.message === "string" ? payload.message : null;
  if (errorCode && message) return `${errorCode}: ${message}`;
  if (message) return message;
  if (errorCode) return errorCode;
  return JSON.stringify(payload, null, 2);
}

// luaToolFailureDetail removes the exact host failure envelope and keeps its Reason text only.
// luaToolFailureDetail 移除精确宿主失败外层结构，仅保留 Reason 错误详情。
function luaToolFailureDetail(output: string): string {
  const match = output.match(
    /^Tool `[^`]+` failed\.\r?\nReason: ([\s\S]*?)\r?\nAdjust the arguments, narrow the scope, or switch to a more appropriate tool before continuing\.$/,
  );
  return match?.[1].trim() ?? output;
}

// luaMarkdownSection extracts one exact H2 section from canonical LuaSkills output.
// luaMarkdownSection 从规范 LuaSkills 输出中提取一个精确二级标题段落。
function luaMarkdownSection(markdown: string, heading: string): string | null {
  const marker = `## ${heading}`;
  const markerIndex = markdown.indexOf(marker);
  if (markerIndex < 0) {
    return null;
  }
  const contentStart = markerIndex + marker.length;
  const remaining = markdown.slice(contentStart).replace(/^\r?\n/, "");
  const nextHeadingIndex = remaining.search(/\r?\n## /);
  return (nextHeadingIndex < 0 ? remaining : remaining.slice(0, nextHeadingIndex)).trim();
}

// luaMarkdownCodeBlocks extracts exact fenced blocks from one canonical LuaSkills section.
// luaMarkdownCodeBlocks 从一个规范 LuaSkills 段落中提取精确围栏代码块。
function luaMarkdownCodeBlocks(section: string): LuaMarkdownCodeBlock[] {
  const blocks: LuaMarkdownCodeBlock[] = [];
  const pattern = /```([^\r\n]*)\r?\n([\s\S]*?)\r?\n```/g;
  for (const match of section.matchAll(pattern)) {
    blocks.push({ language: match[1].trim(), content: match[2] });
  }
  return blocks;
}

// luaJsonObject decodes the exact persistent-session JSON result when it is an object.
// luaJsonObject 解码精确持久会话 JSON 结果，并确保其为对象。
function luaJsonObject(output: string): Record<string, unknown> | null {
  try {
    const value = JSON.parse(output) as unknown;
    return value && typeof value === "object" && !Array.isArray(value)
      ? value as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

// luaJsonDisplayText encodes one successful result as stable indented JSON.
// luaJsonDisplayText 将一个成功结果编码为稳定缩进 JSON。
function luaJsonDisplayText(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

// markdownTableCell escapes one compact value for a GitHub-flavored Markdown table cell.
// markdownTableCell 转义一个紧凑值以用于 GitHub 风格 Markdown 表格单元格。
function markdownTableCell(value: unknown): string {
  if (value === null || value === undefined) {
    return "-";
  }
  return String(value).replace(/\|/g, "\\|").replace(/\r?\n/g, "<br />");
}

// markdownFencedCode wraps exact content with a fence longer than any embedded backtick run.
// markdownFencedCode 使用长于内容内任意反引号序列的围栏包裹精确内容。
function markdownFencedCode(language: "json" | "lua" | "text", content: string): string {
  const longestBacktickRun = Math.max(0, ...(content.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(Math.max(3, longestBacktickRun + 1));
  return `${fence}${language}\n${content}\n${fence}`;
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

// SubagentOrchestrationPresentationCard renders a compact inline subagent dispatch record.
// SubagentOrchestrationPresentationCard 渲染紧凑的内联子代理调度记录。
function SubagentOrchestrationPresentationCard(props: {
  // Subagent orchestration presentation model.
  // 子代理调度展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Whether this row is expanded.
  // 当前行是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for full subagent results.
  // 加载单条隐藏工具详情记录用于完整子代理结果。
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
  const detailCacheKey = subagentOrchestrationPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<SubagentOrchestrationPresentation>(
    () => detailCacheKey ? subagentOrchestrationPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? subagentOrchestrationPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || subagentOrchestrationPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateSubagentOrchestrationPresentation(presentation, payload);
        subagentOrchestrationPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          // A failed read must not populate the successful-detail cache or prevent explicit retries.
          // 失败读取不能填充成功详情缓存，也不能阻止显式重读。
          setHydratedPresentation(presentation);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);
  const handleToggle = React.useCallback(() => {
    if (!subagentPresentationHasExpandableDetail(hydratedPresentation)) {
      return;
    }
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, hydratedPresentation, onExpandedChange, presentation.id]);
  // canExpand prevents empty status rows from opening an empty detail area.
  // canExpand 避免空状态列表行展开出空详情区域。
  const canExpand = subagentPresentationHasExpandableDetail(hydratedPresentation);
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  return (
    <div className={cn("tool-presentation-card subagent-orchestration", canExpand && expanded && "expanded")}>
      <button
        aria-disabled={!canExpand}
        aria-expanded={canExpand && expanded}
        className="tool-presentation-summary subagent-orchestration-summary"
        onClick={handleToggle}
        title={hydratedPresentation.title}
        type="button"
      >
        <Bot className="subagent-orchestration-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command"><span className="tool-presentation-command-text">{subagentCardSummaryText(hydratedPresentation, t)}</span></span>
        <ConversationCollaborationStatus questionId={hydratedPresentation.questionId} executionId={hydratedPresentation.executionId} />
        {canExpand ? (
          expanded ? (
            <ChevronDown className="tool-presentation-toggle" size={15} />
          ) : (
            <ChevronRight className="tool-presentation-toggle" size={15} />
          )
        ) : null}
      </button>
      <ConversationMeasuredCollapse collapseId={`subagent:${presentation.id}`} expanded={canExpand && expanded}>
        <SubagentOrchestrationDetail presentation={hydratedPresentation} t={t} />
      </ConversationMeasuredCollapse>
    </div>
  );
}

// subagentPresentationHasExpandableDetail returns whether a subagent row has useful expanded content.
// subagentPresentationHasExpandableDetail 返回智能体行是否存在有价值的展开内容。
function subagentPresentationHasExpandableDetail(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
): boolean {
  if (
    presentation.operation === "list_agents"
    && presentation.agentsKnown
    && presentation.agents.length === 0
    && !presentation.result
    && !presentation.note
  ) {
    return false;
  }
  return true;
}

// subagentOrchestrationPresentationDetailCacheKey returns a stable cache key for hidden subagent details.
// subagentOrchestrationPresentationDetailCacheKey 返回隐藏子代理详情的稳定缓存键。
function subagentOrchestrationPresentationDetailCacheKey(
  // Subagent presentation model.
  // 子代理展示模型。
  presentation: SubagentOrchestrationPresentation,
): string | null {
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

// SubagentOrchestrationDetail renders the expanded inline body without generic JSON sections.
// SubagentOrchestrationDetail 渲染不包含通用 JSON 区块的内联展开主体。
function SubagentOrchestrationDetail(props: {
  // Presentation to render.
  // 需要渲染的展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  const { presentation, t } = props;
  if (presentation.operation.startsWith("collaboration_")) {
    return <CollaborationToolDetail presentation={presentation} t={t} />;
  }
  if (presentation.operation === "wait") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        <div className="subagent-orchestration-meta">
          <span>{t("conversation.subagent.detail.status")}</span>
          <strong>{subagentWaitStatusText(presentation, t)}</strong>
        </div>
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
      </div>
    );
  }
  if (presentation.operation === "feedback") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : (
          <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.noResult")}</div>
        )}
      </div>
    );
  }
  if (presentation.operation === "agent_orchestration_info") {
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
      </div>
    );
  }
  if (presentation.operation === "list_agents") {
    const emptyOpenAgentStatus = presentation.agentsKnown
      && presentation.agents.length === 0
      && !presentation.result
      && !presentation.note;
    return (
      <div className="subagent-orchestration-detail">
        {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
        {presentation.result ? (
          <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={presentation.result} />
        ) : null}
        {presentation.agents.length ? (
          <div className="subagent-orchestration-agent-list">
            {presentation.agents.map((agent) => (
              <SubagentOrchestrationAgentRow agent={agent} key={agent.id} t={t} />
            ))}
          </div>
        ) : null}
        {emptyOpenAgentStatus ? (
          <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.noOpenAgents")}</div>
        ) : null}
      </div>
    );
  }
  // resultText removes model-facing dispatch acknowledgements from create rows.
  // resultText 从创建行中移除面向模型的调度确认内容。
  const resultText = presentation.operation === "create" ? null : presentation.result ?? null;
  // showResult controls whether model-facing dispatch acknowledgements are visible.
  // showResult 控制是否展示面向模型的调度确认内容。
  const showResult = Boolean(resultText);
  // showStoppedCount records whether the stop count adds useful information to close or interrupt rows.
  // showStoppedCount 记录停止数量是否为关闭或停止行补充了有效信息。
  const showStoppedCount = (presentation.operation === "close" || presentation.operation === "interrupt")
    && presentation.stoppedCount > 0;
  // hasVisibleBody records whether the expanded area has user-facing content.
  // hasVisibleBody 记录展开区是否存在面向用户的内容。
  const hasVisibleBody = Boolean(
    presentation.prompt
    || presentation.effectivePrompt
    || presentation.debugSummary
    || showResult
    || presentation.operation === "close"
    || presentation.operation === "interrupt",
  );
  return (
    <div className="subagent-orchestration-detail">
      {presentation.queued !== null && presentation.queued !== undefined ? (
        <div className="subagent-orchestration-meta">
          <span>{t(presentation.deliveryMode === "interrupt" ? "conversation.subagent.detail.steering" : "conversation.subagent.detail.queued")}</span>
          <strong>{presentation.queued ? t("conversation.subagent.detail.yes") : t("conversation.subagent.detail.no")}</strong>
        </div>
      ) : null}
      {presentation.note ? <div className="subagent-orchestration-note">{presentation.note}</div> : null}
      {presentation.prompt ? (
        <SubagentInlineTextLine
          label={t(presentation.operation === "message" ? "conversation.subagent.detail.message" : "conversation.subagent.detail.prompt")}
          text={presentation.prompt}
        />
      ) : null}
      {presentation.effectivePrompt && presentation.effectivePrompt !== presentation.prompt ? (
        <SubagentInlineTextLine label={t("conversation.subagent.detail.effectivePrompt")} text={presentation.effectivePrompt} />
      ) : null}
      {presentation.debugSummary ? (
        <SubagentInlineTextLine label={t("conversation.subagent.detail.debug")} text={presentation.debugSummary} />
      ) : null}
      {resultText ? (
        <SubagentTextBlock label={t("conversation.subagent.detail.result")} text={resultText} />
      ) : null}
      {presentation.operation === "close" || presentation.operation === "interrupt" ? (
        <>
          {presentation.operation === "close" && (presentation.closedCount > 0 || presentation.closedAgents.length > 0) ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.closedCount")}</span>
              <strong>{presentation.closedCount}</strong>
            </div>
          ) : null}
          {showStoppedCount ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.stoppedCount")}</span>
              <strong>{presentation.stoppedCount}</strong>
            </div>
          ) : null}
          {presentation.clearedMessageCount !== null && presentation.clearedMessageCount !== undefined ? (
            <div className="subagent-orchestration-meta">
              <span>{t("conversation.subagent.detail.clearedMessages")}</span>
              <strong>{presentation.clearedMessageCount}</strong>
            </div>
          ) : null}
        </>
      ) : null}
      {!hasVisibleBody ? (
        <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.empty")}</div>
      ) : null}
    </div>
  );
}

// CollaborationToolDetail keeps question, answer, direction and waiting cards distinct.
// CollaborationToolDetail 保持问题、回答、方向与等待卡片彼此区分。
function CollaborationToolDetail(props: {
  // Exact collaboration presentation model.
  // 精确的协作展示模型。
  presentation: SubagentOrchestrationPresentation;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  const { presentation, t } = props;
  // Question tools expose conversation content, never the internal request/receipt JSON.
  // 问题工具仅展示对话内容，不展示内部请求或回执 JSON。
  const isQuestion = presentation.operation === "collaboration_question"
    || presentation.operation === "collaboration_answer" || presentation.operation === "collaboration_question_wait"
    || presentation.operation === "collaboration_cancel";
  const collaborationState = React.useContext(ConversationCollaborationStateContext);
  const question = presentation.questionId ? collaborationState.questions.get(presentation.questionId) : undefined;
  const answer = question?.question.answer;
  // A newer failed delivery revokes historical answer disclosure as well.
  // 较新的失败交付也撤销历史回答的展示。
  const answerText = answer ? (answer.delivery.state === "failed" ? null : answer.body) : presentation.answerText;
  const argumentText = presentation.collaborationArguments
    ? JSON.stringify(presentation.collaborationArguments, null, 2)
    : null;
  const promptLabel = presentation.operation === "collaboration_answer"
    ? t("conversation.collaboration.detail.answer")
    : presentation.operation === "collaboration_notification"
      ? t("conversation.collaboration.detail.notification")
      : presentation.operation === "collaboration_steer"
      ? t("conversation.subagent.detail.message")
      : t("conversation.collaboration.detail.question");
  return (
    <div className="subagent-orchestration-detail collaboration-tool-detail">
      {presentation.prompt ? <SubagentInlineTextLine label={promptLabel} text={presentation.prompt} /> : null}
      {presentation.deliveryMode ? (
        <div className="subagent-orchestration-meta">
          <span>{t("conversation.subagent.detail.deliveryMode")}</span>
          <strong>{presentation.deliveryMode === "queue" ? t("conversation.subagent.queue.followup") : t("conversation.subagent.queue.steering")}</strong>
        </div>
      ) : null}
      {!isQuestion && argumentText ? <SubagentTextBlock label={t("conversation.collaboration.detail.arguments")} text={argumentText} /> : null}
      {!isQuestion && presentation.result ? <SubagentTextBlock label={t("conversation.collaboration.detail.output")} text={presentation.result} /> : null}
      {isQuestion && presentation.operation !== "collaboration_answer" && answerText ? <SubagentTextBlock label={t("conversation.collaboration.detail.answer")} text={answerText} /> : null}
      {!presentation.prompt && (isQuestion ? !answerText : !argumentText && !presentation.result) ? (
        <div className="subagent-orchestration-empty">{t("conversation.subagent.detail.empty")}</div>
      ) : null}
    </div>
  );
}

// SubagentOrchestrationAgentRow renders one listed child-agent status row.
// SubagentOrchestrationAgentRow 渲染单条已列出的子代理状态行。
function SubagentOrchestrationAgentRow(props: {
  // Agent row model.
  // 代理行模型。
  agent: SubagentOrchestrationAgent;
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string;
}) {
  return (
    <div className="subagent-orchestration-agent">
      <div className="subagent-orchestration-agent-heading">
        <strong>{props.agent.name}</strong>
        <span>{props.agent.status}</span>
      </div>
      {props.agent.debugSummary ? (
        <SubagentInlineTextLine label={props.t("conversation.subagent.detail.debug")} text={props.agent.debugSummary} />
      ) : null}
      {props.agent.result ? (
        <pre className="subagent-orchestration-text">{compactSubagentText(props.agent.result)}</pre>
      ) : (
        <div className="subagent-orchestration-empty">{props.t("conversation.subagent.detail.noResult")}</div>
      )}
    </div>
  );
}

// SubagentInlineTextLine renders one unframed prompt-style line.
// SubagentInlineTextLine 渲染一条无边框的提示词样式文本。
function SubagentInlineTextLine(props: {
  // Line label.
  // 文本行标签。
  label: string;
  // Raw text.
  // 原始文本。
  text: string;
}) {
  return (
    <div className="subagent-orchestration-inline-text">
      <span>{props.label}：</span>
      <code>{compactSubagentText(props.text)}</code>
    </div>
  );
}

// SubagentTextBlock renders one compact prompt or result text block.
// SubagentTextBlock 渲染一段紧凑提示词或结果文本。
function SubagentTextBlock(props: {
  // Block label.
  // 文本块标签。
  label: string;
  // Raw text.
  // 原始文本。
  text: string;
}) {
  return (
    <div className="subagent-orchestration-block">
      <div className="subagent-orchestration-block-label">{props.label}</div>
      <pre className="subagent-orchestration-text">{compactSubagentText(props.text)}</pre>
    </div>
  );
}

// subagentOperationLabel returns the compact operation label.
// subagentOperationLabel 返回紧凑操作文案。
function subagentOperationLabel(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.operation === "collaboration_question") return t("conversation.collaboration.question");
  if (presentation.operation === "collaboration_answer") return t("conversation.collaboration.answer");
  if (presentation.operation === "collaboration_steer") return t("conversation.collaboration.steer");
  if (presentation.operation === "collaboration_question_wait") return t("conversation.collaboration.questionWait");
  if (presentation.operation === "collaboration_notification") return t("conversation.collaboration.notification");
  if (presentation.operation === "collaboration_session_list") return t("conversation.collaboration.sessionList");
  if (presentation.operation === "collaboration_session_read") return t("conversation.collaboration.sessionRead");
  if (presentation.operation === "collaboration_session_wait") return t("conversation.collaboration.sessionWait");
  if (presentation.operation === "collaboration_cancel") return t("conversation.collaboration.cancel");
  if (presentation.operation === "wait") return t("conversation.subagent.detail.wait");
  if (presentation.operation === "message") return t("conversation.subagent.detail.message");
  if (presentation.operation === "agent_orchestration_info") return t("conversation.subagent.detail.templateCollection");
  if (presentation.operation === "list_agents") return t("conversation.subagent.detail.statusCollection");
  if (presentation.operation === "feedback") return t("conversation.subagent.detail.feedback");
  if (presentation.operation === "close") return t("conversation.subagent.detail.closeControl");
  if (presentation.operation === "interrupt") return t("conversation.subagent.detail.interruptControl");
  return t("conversation.subagent.call");
}

// subagentSummaryText returns a terse row summary.
// subagentSummaryText 返回简短行摘要。
function subagentSummaryText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.operation === "collaboration_steer") {
    return presentation.deliveryMode === "queue"
      ? t("conversation.subagent.queue.followup")
      : t("conversation.subagent.queue.steering");
  }
  if (presentation.operation.startsWith("collaboration_")) {
    return subagentOperationLabel(presentation, t);
  }
  if (presentation.operation === "wait") {
    return subagentWaitStatusText(presentation, t);
  }
  if (presentation.operation === "agent_orchestration_info") {
    if (!presentation.agentsKnown) {
      return t("conversation.subagent.detail.templateCollection");
    }
    if (presentation.agents.length === 0) {
      return t("conversation.subagent.detail.templateCollection");
    }
    return `${presentation.agents.length} ${t("conversation.subagent.detail.agent")}`;
  }
  if (presentation.operation === "list_agents") {
    if (!presentation.agentsKnown) {
      return t("conversation.subagent.detail.statusCollection");
    }
    if (presentation.agents.length === 0) {
      return t("conversation.subagent.detail.noOpenAgents");
    }
    return `${presentation.agents.length} ${t("conversation.subagent.detail.agent")}`;
  }
  if (presentation.operation === "feedback") {
    return t("conversation.subagent.detail.openFeedback");
  }
  if (presentation.operation === "message") {
    return presentation.target ?? presentation.title;
  }
  if (presentation.operation === "close") {
    if (presentation.closedCount > 0) {
      return `${presentation.closedCount} ${t("conversation.subagent.detail.closedAgents")}`;
    }
    return presentation.target ?? t("conversation.subagent.detail.closeControl");
  }
  if (presentation.operation === "interrupt") {
    if (presentation.stoppedCount > 0) {
      return presentation.target ?? `${presentation.stoppedCount}`;
    }
    return presentation.target ?? t("conversation.subagent.detail.interruptControl");
  }
  return presentation.title;
}

// subagentCardSummaryText keeps the orchestration operation visible beside its specific target or outcome.
// subagentCardSummaryText 在具体目标或结果旁始终保留调度操作名称。
function subagentCardSummaryText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  // operationLabel is the stable user-facing action that must never be replaced by an agent name.
  // operationLabel 是稳定的用户可见操作，不得被智能体名称替代。
  const operationLabel = subagentOperationLabel(presentation, t);
  // detailText retains the concrete agent, count, or wait result for this invocation.
  // detailText 保留本次调用的具体智能体、数量或等待结果。
  const detailText = subagentSummaryText(presentation, t);
  return detailText === operationLabel ? operationLabel : `${operationLabel} · ${detailText}`;
}

// subagentWaitStatusText maps wait_agent activity to a non-misleading compact label.
// subagentWaitStatusText 将 wait_agent 活动映射为不误导的紧凑文案。
function subagentWaitStatusText(
  // Presentation model.
  // 展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  if (presentation.waitNeedsReply) {
    return t("conversation.subagent.detail.waitNeedsReply");
  }
  if (presentation.waitCheckFailed) {
    return t("conversation.subagent.detail.waitCheckFailed");
  }
  if (presentation.noActiveAgents) {
    return t("conversation.subagent.detail.noPendingWork");
  }
  if (presentation.timedOut) {
    return t("conversation.subagent.detail.waitTimedOut");
  }
  if (presentation.interrupted) {
    return t("conversation.subagent.detail.waitInterrupted");
  }
  if (presentation.terminalEventReceived) {
    return t("conversation.subagent.detail.waitCompleted");
  }
  return t("conversation.subagent.detail.waitPending");
}

// compactSubagentText limits inline subagent text to keep transcript rows light.
// compactSubagentText 限制内联子代理文本长度，让会话行保持轻量。
function compactSubagentText(
  // Source text.
  // 源文本。
  source: string,
): string {
  const normalized = source.trim();
  return normalized.length > 1600 ? `${normalized.slice(0, 1600)}\n...` : normalized;
}

// FILE_CHANGE_OPERATION_LABEL_KEYS maps file-change operations and phases to checked translation keys.
// FILE_CHANGE_OPERATION_LABEL_KEYS 将文件变更操作与阶段映射到受类型检查的翻译键。
const FILE_CHANGE_OPERATION_LABEL_KEYS: Record<FileChangeOperation, Record<"running" | "failed" | "preview" | "completed", TranslationKey>> = {
  create: {
    running: "toolPresentation.file.create.running",
    failed: "toolPresentation.file.create.failed",
    preview: "toolPresentation.file.create.preview",
    completed: "toolPresentation.file.create.completed",
  },
  modify: {
    running: "toolPresentation.file.modify.running",
    failed: "toolPresentation.file.modify.failed",
    preview: "toolPresentation.file.modify.preview",
    completed: "toolPresentation.file.modify.completed",
  },
  delete: {
    running: "toolPresentation.file.delete.running",
    failed: "toolPresentation.file.delete.failed",
    preview: "toolPresentation.file.delete.preview",
    completed: "toolPresentation.file.delete.completed",
  },
  rename: {
    running: "toolPresentation.file.mixed.running",
    failed: "toolPresentation.file.mixed.failed",
    preview: "toolPresentation.file.mixed.preview",
    completed: "toolPresentation.file.mixed.completed",
  },
  mixed: {
    running: "toolPresentation.file.mixed.running",
    failed: "toolPresentation.file.mixed.failed",
    preview: "toolPresentation.file.mixed.preview",
    completed: "toolPresentation.file.mixed.completed",
  },
};

// CommandExecutionPresentationCard renders the command-line specific compact and expanded states.
// CommandExecutionPresentationCard 渲染命令行专用的紧凑与展开状态。
function CommandExecutionPresentationCard(props: {
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

// AskQuestionPresentationCard renders compact and expanded ask/question display.
// AskQuestionPresentationCard 渲染 ask/question 的紧凑与展开展示。
function AskQuestionPresentationCard(props: {
  // Ask/question presentation model.
  // ask/question 展示模型。
  presentation: AskQuestionPresentation;
  // Whether this ask card is expanded.
  // 当前提问卡片是否展开。
  expanded: boolean;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for completed answers.
  // 加载单条隐藏工具详情记录用于补全完成态答案。
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
  const detailCacheKey = askQuestionPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<AskQuestionPresentation>(
    () => detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? askQuestionPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);

  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || askQuestionPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateAskQuestionPresentation(presentation, payload);
        askQuestionPresentationDetailCache.set(detailCacheKey, nextPresentation);
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

  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);
  // status is limited to request formation and active execution.
  // status 仅限请求构造与正在执行阶段。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );

  return (
    <div className={cn("tool-presentation-card ask-question", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary ask-question-summary"
        onClick={handleToggle}
        title={hydratedPresentation.questions.map((question) => question.question).join("\n")}
        type="button"
      >
        <MessageCircleQuestionMark className="ask-question-icon" size={15} />
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command"><span className="tool-presentation-command-text">{hydratedPresentation.title}</span></span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`ask-question:${presentation.id}`} expanded={expanded}>
        <div className="ask-question-detail">
          {hydratedPresentation.questions.map((question) => (
            <AskQuestionAnswerItem
              ignoredAnswerText={hydratedPresentation.ignoredAnswerText}
              item={question}
              key={question.id}
              state={hydratedPresentation.state}
              t={t}
            />
          ))}
        </div>
      </ConversationMeasuredCollapse>
    </div>
  );
}

// askQuestionPresentationDetailCacheKey returns a cache key for one ask detail reference.
// askQuestionPresentationDetailCacheKey 返回单条 ask 详情引用的缓存键。
function askQuestionPresentationDetailCacheKey(presentation: AskQuestionPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return `${presentation.detailRef.recordId ?? ""}:${presentation.detailRef.seq ?? ""}:${presentation.detailRef.messageIndex ?? ""}`;
}

// AskQuestionAnswerItem renders one expanded question and its user answer.
// AskQuestionAnswerItem 渲染展开区中的单个问题与用户回答。
function AskQuestionAnswerItem(props: {
  // Question-answer item.
  // 问答条目。
  item: AskQuestionPresentationItem;
  // Placeholder for ignored answers.
  // 忽略回答时的占位文案。
  ignoredAnswerText: string;
  // Parent presentation state.
  // 父级展示状态。
  state: AskQuestionPresentation["state"];
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  const answerText = askQuestionAnswerText(props.item, props.ignoredAnswerText, props.state, props.t);
  return (
    <div className="ask-question-item">
      <div className="ask-question-row">
        <span className="ask-question-label">{props.t("toolPresentation.ask.questionLabel")}</span>
        <span className="ask-question-text">{props.item.question}</span>
      </div>
      <div className={cn("ask-question-row", !props.item.answered && props.state !== "running" && "ignored")}>
        <span className="ask-question-label">{props.t("toolPresentation.ask.answerLabel")}</span>
        <span className="ask-question-text">{answerText}</span>
      </div>
    </div>
  );
}

// askQuestionAnswerText chooses the visible answer text for running, answered, and ignored states.
// askQuestionAnswerText 为等待中、已回答与忽略状态选择可见答案文本。
function askQuestionAnswerText(
  item: AskQuestionPresentationItem,
  ignoredAnswerText: string,
  state: AskQuestionPresentation["state"],
  t: (key: TranslationKey) => string,
): string {
  if (item.answered && item.answers.length) {
    return item.answers.join(", ");
  }
  if (state === "running") {
    return t("toolPresentation.ask.waitingAnswer");
  }
  return ignoredAnswerText;
}

// askQuestionStatusLabel returns the localized ask lifecycle label.
// askQuestionStatusLabel 返回本地化后的 ask 生命周期文案。
function askQuestionStatusLabel(
  state: ToolPresentationDisplayState,
  t: (key: TranslationKey) => string,
): string {
  if (state === "running") return t("toolPresentation.ask.running");
  if (state === "failed" || state === "stopped") return t("toolPresentation.ask.ignored");
  return t("toolPresentation.ask.completed");
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

// fileTargetLabel combines one exact path with zero or more structured line ranges.
// fileTargetLabel 将一个精确路径与零个或多个结构化行范围组合。
function fileTargetLabel(
  path: string,
  lineRanges: FileAccessLineRangePresentation[],
  t: (key: TranslationKey) => string,
): string {
  // rangeLabel localizes the structured ranges only at the final render boundary.
  // rangeLabel 仅在最终渲染边界对结构化范围执行本地化。
  const rangeLabel = lineRanges
    .map((range) => range.endLine === null
      ? formatTranslation(t, "toolPresentation.fileRead.rangeFrom", { start: String(range.startLine) })
      : formatTranslation(t, "toolPresentation.fileRead.range", {
          end: String(range.endLine),
          start: String(range.startLine),
        }))
    .join(", ");
  return rangeLabel ? `${path} · ${rangeLabel}` : path;
}

// FileAccessPresentationCard renders native read-only file actions without mutation semantics.
// FileAccessPresentationCard 渲染不含变更语义的原生只读文件动作。
function FileAccessPresentationCard(props: {
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
  const canExpand = (resultEvent !== undefined && hydratedPresentation.action !== "read") || (hydratedPresentation.interactive
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
        <div className="file-access-detail">
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
        </div>
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

// FileChangePresentationCard renders compact file operation rows and lazy diff bodies.
// FileChangePresentationCard 渲染紧凑文件操作行与懒加载 diff 内容。
function FileChangePresentationCard(props: {
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
  const [errorExpanded, setErrorExpanded] = React.useState(() => presentation.interactive && presentation.state === "failed" && Boolean(presentation.errorDetail));
  React.useEffect(() => {
    setErrorExpanded(presentation.interactive && presentation.state === "failed" && Boolean(presentation.errorDetail));
  }, [presentation.errorDetail, presentation.id, presentation.interactive, presentation.state]);
  const effectiveExpanded = !displayPresentation.interactive
    ? false
    : displayPresentation.state === "failed" && displayPresentation.errorDetail
      ? errorExpanded
      : expanded;
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
    if (displayPresentation.state === "failed" && displayPresentation.errorDetail) {
      setErrorExpanded((value) => !value);
      return;
    }
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
            noEffectiveEdit
              ? t("toolPresentation.file.notEdited")
              : formatTranslation(t, "toolPresentation.file.count", { count: String(displayPresentation.files.length) })
          }</span>
          <FileChangeCounts added={displayPresentation.totalAdded} presentation={displayPresentation} removed={displayPresentation.totalRemoved} t={t} />
        </span>
        {displayPresentation.interactive ? (effectiveExpanded ? <ChevronDown className="tool-presentation-toggle file-change-toggle" size={15} /> : <ChevronRight className="tool-presentation-toggle file-change-toggle" size={15} />) : <span />}
      </button>
      {displayPresentation.files.length > 0 ? (
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

// FileTextFormatBadge renders encoding and line-ending facts without duplicating gutter ranges.
// FileTextFormatBadge 渲染编码与换行事实，且不重复代码 gutter 的范围。
function FileTextFormatBadge(props: {
  // File-like row whose native text metadata may be available.
  // 可能包含原生文本元数据的类文件行。
  file: Pick<FileChangeFilePresentation, "textFormat">;
  // Translates stable UI copy keys.
  // 翻译稳定 UI 文案键。
  t: (key: TranslationKey) => string;
}) {
  const format = props.file.textFormat;
  if (!format) {
    return null;
  }
  const tokens = [
    format.encoding,
    format.mixedLineEndings
      ? props.t("toolPresentation.file.mixedLineEndings")
      : format.lineEnding,
    format.bom ? "BOM" : null,
  ].filter((value): value is string => Boolean(value));
  return tokens.length
    ? <span className="file-change-text-format">{tokens.join(" · ")}</span>
    : null;
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

// fileChangeStatusLabel returns the localized file-change lifecycle label.
// fileChangeStatusLabel 返回本地化后的文件变更生命周期文案。
function fileChangeStatusLabel(
  presentation: FileChangePresentation,
  operation: FileChangeOperation,
  t: (key: TranslationKey) => string,
): string {
  if (presentation.state === "running") {
    return fileChangeOperationLabel(operation, "running", t);
  }
  if (presentation.hasEffectiveChanges !== null) {
    // execution reports actual execution outcome independently from file changes.
    // execution 独立于文件变更报告真实执行结果。
    const execution = presentation.state === "failed"
      ? t("toolPresentation.file.executionFailed")
      : presentation.state === "stopped"
        ? t("toolPresentation.file.executionStopped")
        : t("toolPresentation.file.executionCompleted");
    // change reports whether the authoritative ChangeSet contains effective edits.
    // change 报告权威 ChangeSet 是否包含有效编辑。
    const change = presentation.hasEffectiveChanges
      ? t("toolPresentation.file.edited")
      : t("toolPresentation.file.notEdited");
    return `${execution} · ${change}`;
  }
  if (presentation.state === "failed") {
    return fileChangeOperationLabel(operation, "failed", t);
  }
  if (presentation.mode === "preview") {
    return fileChangeOperationLabel(operation, "preview", t);
  }
  return fileChangeOperationLabel(operation, "completed", t);
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

// fileChangeOperationLabel maps one operation and phase to a stable translation key.
// fileChangeOperationLabel 将操作与阶段映射为稳定翻译键。
function fileChangeOperationLabel(
  operation: FileChangeOperation,
  phase: "running" | "failed" | "preview" | "completed",
  t: (key: TranslationKey) => string,
): string {
  return t(FILE_CHANGE_OPERATION_LABEL_KEYS[operation][phase]);
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

// formatTranslation replaces simple named placeholders in localized UI templates.
// formatTranslation 替换本地化 UI 模板中的简单具名占位符。
function formatTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
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
