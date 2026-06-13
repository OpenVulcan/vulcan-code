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
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
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
  blockTitle,
  isContextCompactionBlock,
  isMergeableToolBlock,
} from "@/components/conversation/conversation-classifiers";
import type {
  ConversationBlock,
  ConversationBlockKind,
} from "@/components/conversation/conversation-model";

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
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
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
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
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
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  const [block] = props.blocks;
  if (!block) {
    return null;
  }
  if (block.kind === "markdown") {
    return (
      <MarkdownMessage
        content={block.body}
        eager={props.forceEagerMarkdown}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }
  return (
    <SpecialConversationBlock
      autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
      block={block}
      conversationId={props.conversationId}
      itemStreaming={props.itemStreaming}
      parentConversationId={props.parentConversationId}
      sessionRunning={props.sessionRunning}
      onOpenDetail={props.onOpenDetail}
      onOpenMedia={props.onOpenMedia}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      onLoadToolDetail={props.onLoadToolDetail}
      onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
      onTrace={props.onTrace}
    />
  );
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
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
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
  if (props.block.kind === "turn-stats") {
    return <TurnStatsBlock block={props.block} />;
  }

  if (isMergeableToolBlock(props.block)) {
    return (
      <ToolExecutionBlock
        blocks={[props.block]}
        conversationId={props.conversationId}
        parentConversationId={props.parentConversationId}
        sessionRunning={props.sessionRunning}
        onOpenDetail={props.onOpenDetail}
        onOpenMedia={props.onOpenMedia}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
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
        block={props.block}
        sessionRunning={props.sessionRunning}
        streaming={props.itemStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  const Icon = blockIcon(props.block.kind);
  const canOpen = detailEligibleBlockKinds.has(props.block.kind);
  return (
    <button
      className={`special-block ${props.block.kind}`}
      disabled={!canOpen}
      onClick={() => {
        if (canOpen) {
          props.onOpenDetail({
            title: props.block.title,
            kind: props.block.kind,
            body: props.block.body,
            metadata: [
              ["类型", blockTitle(props.block.kind, props.block.toolName)],
              ...(props.block.status ? [["状态", props.block.status] as [string, string]] : []),
              ...(props.block.toolName ? [["工具", props.block.toolName] as [string, string]] : []),
            ],
          });
        }
      }}
      type="button"
    >
      <div className="special-block-main">
        <Icon size={16} />
        <div>
          <strong>{props.block.title}</strong>
          <span>{props.block.summary}</span>
        </div>
      </div>
      {props.block.status ? <StatusPill status={props.block.status} label={props.block.status} /> : null}
      {canOpen ? <Maximize2 size={15} /> : null}
    </button>
  );
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
