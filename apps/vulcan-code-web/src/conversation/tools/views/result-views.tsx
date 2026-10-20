// Shared result bodies: the error the model received, and a Markdown result rendered by the conversation pipeline.
// 共享结果正文：模型收到的错误，以及经会话管线渲染的 Markdown 结果。
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";

/** Shows the error text the model received.
 * 展示模型收到的错误文本。 */
export function ErrorNote(props: { text: string | null }) {
  if (!props.text) return null;
  return <pre className="vk-tool-error">{props.text}</pre>;
}

/** Renders one Markdown result through the shared conversation Markdown pipeline.
 * 通过共享会话 Markdown 管线渲染一条 Markdown 结果。 */
export function MarkdownResult(props: { text: string; error?: boolean }) {
  return (
    <div className="vk-tool-result" data-error={props.error ? "" : undefined}>
      <MarkdownMessage content={props.text} />
    </div>
  );
}
