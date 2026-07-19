import React from "react";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { MarkdownMessage } from "./conversation-markdown";
import {
  useConversationLiveStreamRevision,
  useConversationLiveStreamStore,
} from "@/state/conversation-live-stream-context";
import { conversationLiveStreamKey } from "@/state/conversation-live-stream-store";
import {
  applyStreamingMarkdownDeltas,
  createStreamingMarkdownRenderState,
  streamingMarkdownRevisionRequiresRebase,
  type StreamingMarkdownRenderState,
  type StreamingMarkdownSegment,
} from "./streaming-markdown-render-session";

// StreamingMarkdownDocument renders one cold baseline plus a bounded dynamic tail for an active stream.
// StreamingMarkdownDocument 为一条活动流渲染一个冷基线与受限动态尾部。
// Parameters: props identifies the backend stream and supplies stable fallback content and media behavior.
// 参数：props 标识后端流，并提供稳定回退正文与媒体行为。
// Returns: one segmented streaming Markdown document or a stable fallback renderer.
// 返回值：一个分段流式 Markdown 文档或稳定回退渲染器。
export function StreamingMarkdownDocument(props: {
  // Owning conversation identity used by the live stream Store.
  // 实时流 Store 使用的所属会话身份。
  conversationId: string;
  // Stable streamed message identity.
  // 稳定流式消息身份。
  messageId: string;
  // Structural snapshot body shown only before the Store materializes this stream.
  // 仅在 Store 物化当前流之前展示的结构快照正文。
  fallbackContent: string;
  // Whether a cache-miss fallback should use active streaming Markdown behavior.
  // 缓存缺失回退是否使用活跃流式 Markdown 行为。
  fallbackStreaming: boolean;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  // store owns the canonical Web replica outside global React state.
  // store 在全局 React 状态之外持有规范 Web 副本。
  const store = useConversationLiveStreamStore();
  // streamKey binds this component to one exact streamed message.
  // streamKey 将当前组件绑定到一条精确流式消息。
  const streamKey = React.useMemo(
    () => conversationLiveStreamKey(props.conversationId, props.messageId),
    [props.conversationId, props.messageId],
  );
  // revision updates only when this exact stream changes.
  // revision 仅在当前精确流变化时更新。
  const revision = useConversationLiveStreamRevision(streamKey);
  // renderState owns view-only Markdown segments for this mount.
  // renderState 持有当前挂载周期的纯视图 Markdown 分段。
  const [renderState, setRenderState] = React.useState<StreamingMarkdownRenderState | null>(() => {
    // snapshot is one atomic cold read; no full body is materialized on later ordinary updates.
    // snapshot 是一次原子冷读取；后续普通更新不会再物化完整正文。
    const snapshot = store.readBody(streamKey);
    return snapshot ? createStreamingMarkdownRenderState(snapshot) : null;
  });

  React.useLayoutEffect(() => {
    if (!revision) {
      setRenderState((current) => (
        current && current.streamKey !== streamKey ? null : current
      ));
      return;
    }
    setRenderState((current) => {
      if (!current || streamingMarkdownRevisionRequiresRebase(current, revision)) {
        // snapshot is materialized only for first mount or an authoritative rebase after gap recovery.
        // snapshot 仅在首次挂载或缺口恢复后的权威重基线时物化。
        const snapshot = store.readBody(streamKey);
        return snapshot ? createStreamingMarkdownRenderState(snapshot) : current;
      }
      // deltas contains only generations newer than the mounted render cursor.
      // deltas 只包含比挂载渲染游标更新的 generation。
      const deltas = store.readDeltasAfter(streamKey, current.cursorGeneration);
      if (deltas.length === 0 && revision.lifecycle === current.lifecycle) {
        return current;
      }
      return applyStreamingMarkdownDeltas(current, deltas, revision);
    });
  }, [revision, store, streamKey]);

  if (!renderState) {
    return (
      <MarkdownMessage
        content={props.fallbackContent}
        eager
        streaming={props.fallbackStreaming}
        onOpenMedia={props.onOpenMedia}
      />
    );
  }

  return (
    <div className="streaming-markdown-document" data-stream-lifecycle={renderState.lifecycle}>
      {renderState.baseline ? (
        <MarkdownMessage
          content={renderState.baseline}
          eager
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
      {renderState.committedSegments.map((segment) => (
        <ImmutableStreamingMarkdownSegment
          key={segment.key}
          segment={segment}
          onOpenMedia={props.onOpenMedia}
        />
      ))}
      {renderState.rawPreview ? (
        <pre className="streaming-markdown-raw-preview">
          <code>
            {renderState.tailParts.map((part) => (
              <React.Fragment key={part.key}>{part.content}</React.Fragment>
            ))}
          </code>
        </pre>
      ) : renderState.markdownTail ? (
        <MarkdownMessage
          content={renderState.markdownTail}
          eager
          streaming={renderState.lifecycle === "active"}
          onOpenMedia={props.onOpenMedia}
        />
      ) : null}
    </div>
  );
}

// ImmutableStreamingMarkdownSegment renders one committed segment whose props never change.
// ImmutableStreamingMarkdownSegment 渲染一条 props 永不变化的已封口分段。
// Parameters: props carries one immutable source segment and optional media callback.
// 参数：props 携带一个不可变源码段与可选媒体回调。
// Returns: one memoized Markdown renderer for the committed segment.
// 返回值：一个用于已提交分段的记忆化 Markdown 渲染器。
const ImmutableStreamingMarkdownSegment = React.memo(function ImmutableStreamingMarkdownSegment(props: {
  // Immutable source segment created by the mounted render session.
  // 挂载渲染会话创建的不可变源码分段。
  segment: StreamingMarkdownSegment;
  // Optional media opener used by Markdown images.
  // Markdown 图片使用的可选媒体打开器。
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <MarkdownMessage
      content={props.segment.content}
      eager
      onOpenMedia={props.onOpenMedia}
    />
  );
});
