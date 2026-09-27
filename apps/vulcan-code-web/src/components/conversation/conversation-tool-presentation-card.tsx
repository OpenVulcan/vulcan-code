import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { Button } from "@/components/ui/button";
import type { FileChangeOperation, FileChangePresentation, ToolPresentation, ToolPresentationDisplayState } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { ToolResultEventContext, type ToolRequestCardProgress, type ToolDetailLoader } from "./tool-presentations/tool-card-common";
import { FileChangePresentationCard } from "./tool-presentations/file-change-card";
import { FileAccessPresentationCard } from "./tool-presentations/file-access-card";
import { MediaReadPresentationCard } from "./tool-presentations/media-read-card";
import { CommandExecutionPresentationCard } from "./tool-presentations/command-execution-card";
import { AskQuestionPresentationCard } from "./tool-presentations/ask-question-card";
import { SubagentOrchestrationPresentationCard } from "./tool-presentations/subagent-orchestration-card";
import { LuaExecutionPresentationCard } from "./tool-presentations/lua-execution-card";
import { MarkdownToolPresentationCard } from "./tool-presentations/markdown-tool-card";
import { ProviderToolStructuredPresentationCard, ProviderToolTextPresentationCard, ProviderToolExtractPresentationCard, ProviderToolSearchPresentationCard } from "./tool-presentations/provider-tool-cards";
import { ModelExtensionToolPresentationCard, NativeSemanticToolPresentationCard, GenericToolMarkdownPresentationCard } from "./tool-presentations/semantic-tool-cards";

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
  if (props.presentation.kind === "media_read") {
    return <MediaReadPresentationCard presentation={props.presentation} />;
  }
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

// fileChangeOperationLabel maps one operation and phase to a stable translation key.
// fileChangeOperationLabel 将操作与阶段映射为稳定翻译键。
function fileChangeOperationLabel(
  operation: FileChangeOperation,
  phase: "running" | "failed" | "preview" | "completed",
  t: (key: TranslationKey) => string,
): string {
  return t(FILE_CHANGE_OPERATION_LABEL_KEYS[operation][phase]);
}
