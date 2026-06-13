import { Wrench } from "lucide-react";
import {
  GeneratedMediaStrip,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import type {
  ConversationDetail,
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { cn } from "@/lib/utils";
import {
  mergeGeneratedMediaAttachments,
  toolExecutionMayHaveGeneratedMedia,
  useToolDetailMediaAttachments,
} from "./conversation-tool-execution-media";
import {
  buildToolExecutionDetailBody,
  preferredTaskSessionDisplayBlock,
  toolDisplayStateLabel,
  toolExecutionDetailRef,
  toolExecutionDisplayState,
  toolExecutionErrorPreview,
  toolExecutionIdentifier,
  toolExecutionMediaAttachments,
  toolExecutionName,
  toolExecutionSummary,
} from "./conversation-tool-execution-model";
import type {
  ToolExecutionConversationBlock,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";

export type {
  ToolExecutionConversationBlock,
  ToolExecutionConversationBlockKind,
  ToolExecutionTraceCallback,
} from "./conversation-tool-execution-types";
export { toolExecutionDisplayState } from "./conversation-tool-execution-model";

// ToolExecutionBlock renders one compact merged tool execution row.
// ToolExecutionBlock 渲染单条紧凑的合并工具执行行。
export function ToolExecutionBlock(props: {
  // Tool-related blocks merged into one compact row.
  // 合并成单条紧凑行的工具相关块。
  blocks: ToolExecutionConversationBlock[];
  // Conversation id that owns hidden detail records.
  // 拥有隐藏详情记录的会话标识。
  conversationId: string | null;
  // Parent conversation id used by routed subagent navigation.
  // 已路由子代理导航使用的父会话标识。
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
  // Extracts generated media attachments from one hidden detail payload.
  // 从单条隐藏详情载荷中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local UI diagnostics.
  // 用于本地 UI 诊断的可选埋点回调。
  onTrace?: ToolExecutionTraceCallback;
}) {
  const callBlock = props.blocks.find((block) => block.kind === "tool-call");
  const resultBlock = props.blocks.find((block) => block.kind === "tool-result");
  const commandBlock = props.blocks.find((block) => block.kind === "command");
  const primaryBlock = preferredTaskSessionDisplayBlock(props.blocks) ?? resultBlock ?? commandBlock ?? callBlock ?? props.blocks[0];
  if (!primaryBlock) {
    return null;
  }

  const summary = toolExecutionSummary(props.blocks, props.sessionRunning);
  const toolName = toolExecutionName(props.blocks);
  const displayState = toolExecutionDisplayState(props.blocks, props.sessionRunning);
  const mediaAttachments = toolExecutionMediaAttachments(props.blocks);
  const taskSessionId = props.blocks.find((block) => block.taskSessionId)?.taskSessionId ?? null;
  const identifier = toolExecutionIdentifier(props.blocks);
  const errorPreview = toolExecutionErrorPreview(props.blocks);
  const detailRefFields = toolExecutionDetailRef(props.blocks);
  const detailRef = props.conversationId && (detailRefFields || primaryBlock.toolCallId)
    ? { sessionId: props.conversationId, ...(detailRefFields ?? {}), ...(primaryBlock.toolCallId ? { toolCallId: primaryBlock.toolCallId } : {}) }
    : undefined;
  const hydratedMediaAttachments = useToolDetailMediaAttachments(
    detailRef,
    mediaAttachments.length === 0 && toolExecutionMayHaveGeneratedMedia(toolName),
    props.onLoadToolDetail,
    props.onExtractGeneratedMediaAttachments,
  );
  const displayedMediaAttachments = mergeGeneratedMediaAttachments(mediaAttachments, hydratedMediaAttachments);
  const metadata = [
    ...(toolName ? [["工具", toolName] as [string, string]] : []),
    ...(identifier ? [["标识", identifier] as [string, string]] : []),
    ...(errorPreview ? [["错误", errorPreview] as [string, string]] : []),
    ["状态", toolDisplayStateLabel(displayState)] as [string, string],
    ...(resultBlock ? [["结果", "已返回"] as [string, string]] : []),
  ];

  // openFallbackDetail builds the full modal body lazily to keep transcript rows cheap.
  // openFallbackDetail 延迟构建完整弹窗正文，使转录行保持轻量。
  const openFallbackDetail = () => {
    props.onTrace?.("tool.open_fallback_detail", {
      toolName,
      displayState,
      primaryKind: primaryBlock.kind,
      taskSessionId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    props.onOpenDetail({
      title: summary,
      kind: primaryBlock.kind,
      body: buildToolExecutionDetailBody(props.blocks),
      detailRef,
      metadata,
      mediaAttachments: displayedMediaAttachments,
    });
  };
  // openDetail prefers the routed subagent panel when this tool row represents a child task.
  // openDetail 在工具行代表子任务时优先打开已路由子代理面板。
  const openDetail = () => {
    props.onTrace?.("tool.open", {
      toolName,
      displayState,
      taskSessionId,
      parentConversationId: props.parentConversationId,
      blockKinds: props.blocks.map((block) => block.kind),
    });
    if (props.parentConversationId && taskSessionId) {
      void props.onOpenSubagentPanel(props.parentConversationId, taskSessionId).then((opened) => {
        props.onTrace?.("tool.open_subagent_result", {
          toolName,
          taskSessionId,
          opened,
          parentConversationId: props.parentConversationId,
        });
        if (!opened) {
          openFallbackDetail();
        }
      });
      return;
    }
    openFallbackDetail();
  };

  return (
    <div className="tool-execution-stack">
      <button
        className={cn("tool-execution-row", displayState === "running" && "running")}
        onClick={openDetail}
        type="button"
      >
        <Wrench size={15} />
        <span>{summary}</span>
      </button>
      {displayedMediaAttachments.length > 0 ? (
        <GeneratedMediaStrip
          attachments={displayedMediaAttachments}
          onOpenDetail={openDetail}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}
