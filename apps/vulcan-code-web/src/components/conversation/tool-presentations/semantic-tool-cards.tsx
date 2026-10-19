// Model extension, native semantic and generic Markdown tool cards.
// 模型扩展、原生语义与通用 Markdown 工具卡。
import * as React from "react";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { Button } from "@/ui-kit/primitives/button";
import { Braces, ChevronDown, ChevronRight, Code2, Hammer, ListChecks, Target } from "lucide-react";
import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationToolResultTabs } from "../conversation-tool-result-tabs";
import { ConversationTodoResult } from "../conversation-todo-result";
import { ConversationMediaGenerationResult, isMediaGenerationTool } from "../conversation-media-generation-result";
import { ModelExtensionToolPayloadCards } from "@/components/conversation/conversation-model-extension-tool-payloads";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import type { GenericToolMarkdownPresentation, NativeSemanticToolPresentation } from "@/components/conversation/conversation-tool-presentations";
import { modelExtensionToolCapability, modelExtensionToolMarkdown } from "@/components/conversation/tool-presentations/model-extension-tool-presentation";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import { ToolResultBody, type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel } from "./tool-card-common";

// ModelExtensionToolPresentationCard renders one extension call with independent input and output cards.
// ModelExtensionToolPresentationCard 使用相互独立的输入卡与返回卡渲染一次扩展调用。
// Parameters: props carries the localized extension model, active state and optional result footer.
// 参数：props 携带本地化扩展模型、活动状态及可选返回页脚。
// Returns: a stable tool header followed by the available independently folded payload cards.
// 返回值：稳定工具标题，以及当前可用且可独立折叠的载荷卡片。
export function ModelExtensionToolPresentationCard(props: {
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
export function NativeSemanticToolPresentationCard(props: {
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
export function GenericToolMarkdownPresentationCard(props: {
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
  const LazyDomain = React.useMemo(() => React.lazy(() => import("../conversation-branch-tool-domain")), [attempt]);
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
