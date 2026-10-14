import React from "react";
import { MessageSquareReply, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n, type Translate } from "@/i18n";
import "@/styles/answer-components.css";
import type { AnswerReplyEntry, AnswerReplyStore } from "./answer-reply";

/** Summarize prepared replies as Markdown for the composer: decisions with their picks (a recommended pick is
 * labelled), then comments with the commented component; notes follow as quotes.
 * 将已准备的回复汇总为供输入区使用的 Markdown：先列出决策及所选项（建议项附注），再列出批注及对应组件；说明以引用跟随。 */
export function formatAnswerReply(entries: AnswerReplyEntry[], t: Translate): string {
  const recommended = t("answerComponents.decision.recommended");
  const quote = (note: string) => note.trim() ? note.trim().split(/\r?\n/u).map((line) => `  > ${line}`.trimEnd()) : [];
  const decisions = entries.flatMap((entry) => entry.kind === "decision" ? [
    `- ${entry.question} → ${entry.picks.length > 0
      ? entry.picks.map((pick) => entry.recommended.includes(pick) ? `${pick} (${recommended})` : pick).join(", ")
      : t("answerComponents.reply.noChoice")}`,
    ...quote(entry.note),
  ] : []);
  const comments = entries.flatMap((entry) => entry.kind === "comment" ? [`- ${entry.subject}`, ...quote(entry.note)] : []);
  return [
    ...(decisions.length > 0 ? [`**${t("answerComponents.reply.decisions")}**`, ...decisions] : []),
    ...(decisions.length > 0 && comments.length > 0 ? [""] : []),
    ...(comments.length > 0 ? [`**${t("answerComponents.reply.comments")}**`, ...comments] : []),
  ].join("\n");
}

/** Strip above the composer that counts prepared replies and puts them into the draft; it never sends.
 * 输入区上方的条带：统计已准备的回复并将其放入草稿；从不发送。 */
export function AnswerReplyTray({ onInsert, store }: { onInsert: (text: string) => void; store: AnswerReplyStore }) {
  const { t } = useI18n();
  React.useSyncExternalStore(store.subscribe, store.size);
  const entries = store.list();
  const decisions = entries.filter((entry) => entry.kind === "decision").length;
  const comments = entries.length - decisions;
  return (
    <div className="answer-reply-tray">
      <MessageSquareReply aria-hidden="true" size={15} />
      <span className="answer-reply-tray-counts">
        {decisions > 0 ? <span>{t("answerComponents.reply.decisions")} <b>{decisions}</b></span> : null}
        {comments > 0 ? <span>{t("answerComponents.reply.comments")} <b>{comments}</b></span> : null}
      </span>
      <Button className="answer-reply-tray-insert" onClick={() => {
        // Read at click time: an edited note changes an entry without changing the counts this strip renders.
        // 在点击时读取：修改说明会改变条目，但不会改变本条带显示的计数。
        onInsert(formatAnswerReply(store.list(), t));
        store.clear();
      }} size="sm" type="button" variant="ghost">
        {t("answerComponents.reply.insert")}
      </Button>
      <Button aria-label={t("answerComponents.reply.discard")} className="answer-reply-tray-discard" onClick={() => store.clear()}
        size="icon-sm" title={t("answerComponents.reply.discard")} type="button" variant="ghost">
        <Trash2 size={14} />
      </Button>
    </div>
  );
}
