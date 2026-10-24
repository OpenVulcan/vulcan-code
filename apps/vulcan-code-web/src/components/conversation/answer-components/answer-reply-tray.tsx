import React from "react";
import { MessageSquareReply, Trash2 } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { useI18n } from "@/i18n";
import "@/ui-kit/styles/answer.css";
import type { AnswerReplyStore } from "./answer-reply";
import { formatAnswerReply } from "./answer-reply-format";

/** Strip above the composer that counts prepared replies and puts them into the draft; it never sends.
 * 输入区上方的条带：统计已准备的回复并将其放入草稿；从不发送。 */
export function AnswerReplyTray({ store }: { store: AnswerReplyStore }) {
  const { t } = useI18n();
  React.useSyncExternalStore(store.subscribe, store.size);
  const entries = store.list();
  const decisions = entries.filter((entry) => entry.kind === "decision").length;
  const comments = entries.length - decisions;
  return (
    <div className="vk-a-reply-tray">
      <MessageSquareReply aria-hidden="true" size={15} />
      <span className="vk-a-reply-tray-counts">
        {decisions > 0 ? <span>{t("answerComponents.reply.decisions")} <b>{decisions}</b></span> : null}
        {comments > 0 ? <span>{t("answerComponents.reply.comments")} <b>{comments}</b></span> : null}
      </span>
      <Button className="vk-a-reply-tray-insert" onClick={() => {
        // Formatted at click time: an edited note changes an entry without changing the counts this strip renders.
        // 在点击时格式化：修改说明会改变条目，但不会改变本条带显示的计数。
        store.deliver("insert", (prepared) => formatAnswerReply(prepared, t));
      }} size="sm" type="button" variant="ghost">
        {t("answerComponents.reply.insert")}
      </Button>
      <Button aria-label={t("answerComponents.reply.discard")} className="vk-a-reply-tray-discard" onClick={() => store.clear()}
        size="icon-sm" title={t("answerComponents.reply.discard")} type="button" variant="ghost">
        <Trash2 size={14} />
      </Button>
    </div>
  );
}
