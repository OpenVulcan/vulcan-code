import {
  BrainCircuit,
  FileText,
  Hammer,
  Maximize2,
  ScrollText,
  TerminalSquare,
  type LucideIcon,
} from "lucide-react";

import { StatusPill } from "@/components/shared/status";
import { ModelSwitchNotice } from "./conversation-model-switch";
import { GoalStatusNotice } from "./conversation-goal-notice";
import { metadataString } from "./conversation-metadata";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { StreamingMarkdownDocument } from "@/components/conversation/streaming-markdown-document";
import {
  type ConversationDetail,
  type ConversationDetailRef,
  type ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  ContextCompactionBlock,
  ThinkingBlock,
  TurnStatsBlock,
} from "@/components/conversation/conversation-blocks";
import { ToolExecutionBlock } from "@/components/conversation/conversation-tool-execution";
import {
  PlanArtifactBlock,
  type PlanArtifactActionPayload,
} from "@/components/conversation/conversation-plan-artifact";
import {
  parseSystemProviderError,
  SystemErrorCard,
} from "@/components/conversation/conversation-system-error-card";
import {
  providerRetryDisplayState,
  ProviderRetryCard,
} from "@/components/conversation/conversation-provider-retry-card";
import {
  blockTitle,
  isContextCompactionBlock,
  isMergeableToolBlock,
} from "@/components/conversation/conversation-classifiers";
import { useI18n } from "@/i18n";
import type {
  ConversationBlock,
  ConversationBlockKind,
  ConversationPlanArtifact,
} from "@/components/conversation/conversation-model";
import type { CommandSessionSummary } from "@/types";

// ConversationBlocksView dispatches parsed blocks to their visual renderer.
// ConversationBlocksView 将解析块分派给对应视觉渲染器。
export function ConversationBlocksView(props: {
  // Thinking block that should auto expand while running.
  // 运行中应自动展开的思考块。
  autoExpandThinkingBlockId: string | null;
  // Blocks rendered in this projected row.
  // 当前投影行中渲染的块。
  blocks: ConversationBlock[];
  // Conversation id used for hidden tool details.
  // 隐藏工具详情使用的会话标识。
  conversationId: string | null;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Force Markdown blocks in externally hidden panels to stay mounted when visible.
  // 强制外层隐藏面板中的 Markdown 块在可见时保持直接挂载。
  forceEagerMarkdown?: boolean;
  // Whether this row is currently streaming.
  // 该行当前是否正在流式增长。
  itemStreaming: boolean;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Currently pending Plan artifact, when user confirmation is required.
  // 需要用户确认时的当前待处理计划产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Executes one pending Plan artifact.
  // 执行单个待确认计划产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion for one pending Plan artifact.
  // 为单个待确认计划产物发送补充建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels one pending Plan artifact.
  // 取消单个待确认计划产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  if (props.blocks.length > 1 && props.blocks.every(isMergeableToolBlock)) {
    return (
      <ToolExecutionBlock
        blocks={props.blocks}
        conversationId={props.conversationId}
        expandedToolPresentationIds={props.expandedToolPresentationIds}
        workspacePath={props.workspacePath}
        commandSessions={props.commandSessions}
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
        onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
        onTrace={props.onTrace}
      />
    );
  }
  if (props.blocks.some(isContextCompactionBlock)) {
    return (
      <ContextCompactionBlock
        blocks={props.blocks}
        conversationId={props.conversationId}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  // renderSpecialBlock keeps shared row props identical when a secondary structured block owns the visible card.
  // renderSpecialBlock 在次级结构化块承载可见卡片时保持行级共享属性一致。
  const renderSpecialBlock = (block: ConversationBlock) => (
    <SpecialConversationBlock
      autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
      block={block}
      conversationId={props.conversationId}
      expandedToolPresentationIds={props.expandedToolPresentationIds}
      workspacePath={props.workspacePath}
      commandSessions={props.commandSessions}
      itemStreaming={props.itemStreaming}
      parentConversationId={props.parentConversationId}
      sessionRunning={props.sessionRunning}
      onOpenDetail={props.onOpenDetail}
      onOpenMedia={props.onOpenMedia}
      onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      onLoadToolDetail={props.onLoadToolDetail}
      onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
      onCopyText={props.onCopyText}
      pendingPlanArtifact={props.pendingPlanArtifact}
      onPlanArtifactExecute={props.onPlanArtifactExecute}
      onPlanArtifactSuggest={props.onPlanArtifactSuggest}
      onPlanArtifactCancel={props.onPlanArtifactCancel}
      onTrace={props.onTrace}
    />
  );
  // planArtifactBlock takes precedence because submit_plan stores the artifact on the tool row metadata.
  // planArtifactBlock 优先展示，因为 submit_plan 会把计划产物挂在工具行元数据上。
  const planArtifactBlock = props.blocks.find((block) => block.kind === "plan-artifact" && block.planArtifact);
  if (planArtifactBlock) {
    return renderSpecialBlock(planArtifactBlock);
  }

  const [block] = props.blocks;
  if (!block) {
    return null;
  }
  if (block.kind === "markdown") {
    if (props.itemStreaming && props.conversationId && block.sourceMessageId) {
      return (
        <StreamingMarkdownDocument
          conversationId={props.conversationId}
          fallbackContent={block.body}
          fallbackStreaming
          messageId={block.sourceMessageId}
          onOpenMedia={props.onOpenMedia}
        />
      );
    }
    return (
      <MarkdownMessage
        content={block.body}
        eager={props.forceEagerMarkdown}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }
  return renderSpecialBlock(block);
}

// SpecialConversationBlock renders compact tool, command, diff, thinking, and notice rows.
// SpecialConversationBlock 渲染紧凑的工具、命令、diff、思考与提示行。
function SpecialConversationBlock(props: {
  // Thinking block that should auto expand while running.
  // 运行中应自动展开的思考块。
  autoExpandThinkingBlockId: string | null;
  // Block rendered by this compact row.
  // 当前紧凑行渲染的块。
  block: ConversationBlock;
  // Conversation id used for hidden tool details.
  // 隐藏工具详情使用的会话标识。
  conversationId: string | null;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Whether this row is currently streaming.
  // 该行当前是否正在流式增长。
  itemStreaming: boolean;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
  // Currently pending Plan artifact, when user confirmation is required.
  // 需要用户确认时的当前待处理计划产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Executes one pending Plan artifact.
  // 执行单个待确认计划产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion for one pending Plan artifact.
  // 为单个待确认计划产物发送补充建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels one pending Plan artifact.
  // 取消单个待确认计划产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  const { t } = useI18n();
  // systemProviderError keeps canonical provider failures inside the existing transcript card surface.
  // systemProviderError 让规范 Provider 失败保持在现有转录卡片界面内。
  const systemProviderError = props.block.kind === "notice"
    ? parseSystemProviderError(props.block.body)
    : null;
  if (systemProviderError) {
    return <SystemErrorCard error={systemProviderError} />;
  }
  // providerRetryState exists only for backend-owned Provider retry lifecycle notes.
  // providerRetryState 仅存在于后端持有的 Provider 重试生命周期提示。
  const providerRetryState = props.block.kind === "notice"
    ? providerRetryDisplayState(props.block.rawMetadata)
    : null;
  if (providerRetryState) {
    return <ProviderRetryCard block={props.block} state={providerRetryState} />;
  }

  // The V1 adapter preserves before/after labels in this exact notice category.
  // V1 适配器在此精确提示类别中保留前后显示标签。
  if (props.block.kind === "notice" && metadataString(props.block.rawMetadata, ["notice_kind"]) === "model_switch") {
    return <ModelSwitchNotice summary={props.block.summary} />;
  }
  if (props.block.kind === "notice" && metadataString(props.block.rawMetadata, ["notice_kind"]) === "goal_status") {
    return <GoalStatusNotice reasonCode={metadataString(props.block.rawMetadata, ["reason_code"]) ?? ""} />;
  }

  if (props.block.kind === "turn-stats") {
    return <TurnStatsBlock block={props.block} />;
  }
  if (props.block.kind === "plan-artifact" && props.block.planArtifact) {
    const pending = planArtifactsMatch(props.block.planArtifact, props.pendingPlanArtifact);
    return (
      <PlanArtifactBlock
        artifact={props.block.planArtifact}
        disabled={props.sessionRunning}
        pending={pending}
        onCancel={props.onPlanArtifactCancel ?? (() => undefined)}
        onCopyText={props.onCopyText}
        onExecute={props.onPlanArtifactExecute ?? (() => undefined)}
        onSuggest={props.onPlanArtifactSuggest ?? (() => undefined)}
      />
    );
  }

  if (isMergeableToolBlock(props.block)) {
    return (
      <ToolExecutionBlock
        blocks={[props.block]}
        conversationId={props.conversationId}
        expandedToolPresentationIds={props.expandedToolPresentationIds}
        workspacePath={props.workspacePath}
        commandSessions={props.commandSessions}
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
        onLoadToolDetail={props.onLoadToolDetail}
        onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
        onTrace={props.onTrace}
      />
    );
  }
  if (props.block.kind === "thinking") {
    return (
      <ThinkingBlock
        autoExpanded={props.block.id === props.autoExpandThinkingBlockId}
        autoExpandWhileStreaming={props.block.id === props.autoExpandThinkingBlockId}
        block={props.block}
        conversationId={props.conversationId}
        sessionRunning={props.sessionRunning}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  const Icon = blockIcon(props.block.kind);
  const canOpen = detailEligibleBlockKinds.has(props.block.kind);
  const displayTitle = localizedConversationBlockTitle(props.block, t);
  return (
    <button
      className={`special-block ${props.block.kind}`}
      disabled={!canOpen}
      onClick={() => {
        if (canOpen) {
          props.onOpenDetail({
            title: displayTitle,
            kind: props.block.kind,
            body: props.block.body,
            metadata: [
              [t("conversation.metadata.type"), blockTitle(props.block.kind, props.block.toolName, t)],
              ...(props.block.status ? [[t("conversation.metadata.status"), props.block.status] as [string, string]] : []),
              ...(props.block.toolName ? [[t("conversation.metadata.tool"), props.block.toolName] as [string, string]] : []),
            ],
          });
        }
      }}
      type="button"
    >
      <div className="special-block-main">
        <Icon size={16} />
        <div>
          <strong>{displayTitle}</strong>
          <span>{props.block.summary}</span>
        </div>
      </div>
      {props.block.status ? <StatusPill status={props.block.status} label={props.block.status} /> : null}
      {canOpen ? <Maximize2 size={15} /> : null}
    </button>
  );
}

// localizedConversationBlockTitle preserves explicit transcript titles and localizes parser defaults.
// localizedConversationBlockTitle 保留显式转录标题，并本地化解析器默认标题。
function localizedConversationBlockTitle(
  block: ConversationBlock,
  t: (key: import("@/i18n-messages").TranslationKey) => string,
): string {
  const defaultTitle = blockTitle(block.kind, block.toolName);
  return block.title === defaultTitle ? blockTitle(block.kind, block.toolName, t) : block.title;
}

// planArtifactsMatch decides whether one transcript Plan card owns the current pending decision.
// planArtifactsMatch 判断单个转录计划卡是否承载当前待处理决策。
function planArtifactsMatch(
  artifact: ConversationPlanArtifact,
  pendingArtifact: ConversationPlanArtifact | null | undefined,
): boolean {
  if (!pendingArtifact) {
    return false;
  }
  if (artifact.path && artifact.updatedAt && artifact.path === pendingArtifact.path && artifact.updatedAt === pendingArtifact.updatedAt) {
    return true;
  }
  return artifact.name === pendingArtifact.name && artifact.content.trim() === pendingArtifact.content.trim();
}

// detailEligibleBlockKinds lists block types that should open a dedicated detail modal.
// detailEligibleBlockKinds 列出应打开专用详情弹窗的块类型。
const detailEligibleBlockKinds = new Set<ConversationBlockKind>(["tool-call", "tool-result", "command", "diff", "notice"]);

// blockIcon returns the icon component used by one special transcript block.
// blockIcon 返回单个特殊转录块使用的图标组件。
function blockIcon(kind: ConversationBlockKind): LucideIcon {
  if (kind === "diff") return FileText;
  if (kind === "thinking") return BrainCircuit;
  if (kind === "command") return TerminalSquare;
  if (kind === "tool-call" || kind === "tool-result") return Hammer;
  return ScrollText;
}
