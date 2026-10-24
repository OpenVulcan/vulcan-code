import type { Translate } from "@/i18n";
import type { AnswerReplyEntry } from "./answer-reply";

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
