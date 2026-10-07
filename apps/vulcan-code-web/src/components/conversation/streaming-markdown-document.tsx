import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { MarkdownMessage } from "./conversation-markdown";

// StreamingMarkdownDocument renders the current canonical V1 event body with the restored Markdown surface.
// StreamingMarkdownDocument 使用恢复后的 Markdown 界面渲染当前规范 V1 事件正文。
// Parameters: props supplies stable event identity, canonical content, stream state, and media behavior.
// 参数：props 提供稳定事件身份、规范正文、流状态与媒体行为。
// Returns: one Markdown document whose body is replaced atomically by V1 event revisions.
// 返回值：正文随 V1 事件修订原子替换的一份 Markdown 文档。
export function StreamingMarkdownDocument(props: {
  // Owning conversation identity retained for stable DOM diagnostics.
  // 为稳定 DOM 诊断保留的所属会话身份。
  conversationId: string;
  // Stable V1 event-derived presentation identity.
  // 从 V1 事件派生的稳定展示身份。
  messageId: string;
  // Canonical body from the latest V1 event revision.
  // 来自最新 V1 事件修订的规范正文。
  fallbackContent: string;
  // Whether the canonical event remains in a streaming state.
  // 规范事件是否仍处于流式状态。
  fallbackStreaming: boolean;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <div
      className="streaming-markdown-document"
      data-conversation-id={props.conversationId}
      data-message-id={props.messageId}
      data-stream-lifecycle={props.fallbackStreaming ? "active" : "sealed"}
    >
      <MarkdownMessage
        content={props.fallbackContent}
        sourceMessageId={props.messageId}
        eager
        onOpenMedia={props.onOpenMedia}
        streaming={props.fallbackStreaming}
      />
    </div>
  );
}
