import React from "react";
import { Check, Copy } from "lucide-react";
import { useI18n } from "@/i18n";
import { runConversationControlAction } from "../conversation-control-feedback";

/** Clipboard service shared by framed blocks; rejects when the browser denies it.
 * 框架型区块共用的剪贴板服务；浏览器拒绝时抛出。 */
export interface AnswerCopyHost {
  copy: (text: string) => Promise<void>;
}

/** Extra action a framed answer component shows beside its own actions, such as the reader's comment button.
 * 框架型回答组件在自身操作旁显示的额外操作，例如读者的批注按钮。 */
export const AnswerFrameActionContext = React.createContext<React.ReactNode>(null);

/** Framed tabular block whose copy-as-Markdown action appears on hover. Plain Markdown tables use it
 * at startup, so it stays outside the deferred component chunk.
 * 框架型表格区块，悬停时显示「复制为 Markdown」操作。普通 Markdown 表格在启动时即使用它，
 * 因此不放入延迟加载的组件分块。 */
export function AnswerFrame({ children, copy, host }: { children: React.ReactNode; copy: string; host: AnswerCopyHost }) {
  const { t } = useI18n();
  const extraAction = React.useContext(AnswerFrameActionContext);
  return (
    <section className="answer-frame" data-hover-actions="true">
      <span className="answer-hover-actions">
        {extraAction}
        <CopyAction host={host} label={t("answerComponents.copyMarkdown")} text={copy} />
      </span>
      {children}
    </section>
  );
}

/** Icon button that copies `text` and briefly confirms.
 * 复制 `text` 并短暂确认的图标按钮。 */
export function CopyAction({ host, label, text }: { host: AnswerCopyHost; label: string; text: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = React.useState(false);
  React.useEffect(() => {
    if (!copied) return undefined;
    const timer = window.setTimeout(() => setCopied(false), 1200);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const title = copied ? t("markdown.copied") : label;
  return (
    <button aria-label={title} className="markdown-code-action" title={title} type="button"
      onClick={() => {
        setCopied(false);
        void runConversationControlAction(() => host.copy(text), t("conversation.control.copyFailed")).then(setCopied);
      }}>
      {copied ? <Check size={14} /> : <Copy size={14} />}
    </button>
  );
}
