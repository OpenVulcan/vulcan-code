// Provider-hosted tool cards: structured, text, extract and search results.
// 供应商托管工具卡：结构化、文本、提取与搜索结果。
import * as React from "react";
import { Braces, ChevronDown, ChevronRight, FileText, Search } from "lucide-react";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import type { ProviderToolExtractPresentation, ProviderToolSearchPresentation, ProviderToolStructuredPresentation, ProviderToolTextPresentation } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { ToolResultBody, type ToolRequestCardProgress, activeToolStatusLabel } from "./tool-card-common";

// ProviderToolStructuredPresentationCard renders unified embedding, rerank, and composite output.
// ProviderToolStructuredPresentationCard 渲染统一向量、重排与复合输出。
export function ProviderToolStructuredPresentationCard(props: {
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
export function ProviderToolTextPresentationCard(props: {
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
export function ProviderToolExtractPresentationCard(props: {
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
export function ProviderToolSearchPresentationCard(props: {
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
