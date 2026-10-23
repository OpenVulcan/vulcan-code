// Ask card. The live questionnaire is answered in its own panel; the card keeps the record: each question with its
// options and the answer the user gave.
// 提问卡。实时问卷在独立面板中作答；卡片保留记录：每个问题及其选项，以及用户给出的回答。
import * as React from "react";
import { MessageCircleQuestion } from "lucide-react";
import { useI18n } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, objectOf, textOf } from "../tool-call";
import { ErrorNote } from "../views/result-views";

// AskQuestion is one question of a questionnaire.
// AskQuestion 是问卷中的一个问题。
export interface AskQuestion {
  // Short header.
  // 简短标题。
  header: string;
  // Full question.
  // 完整问题。
  question: string;
  // Offered option labels.
  // 提供的选项标签。
  options: string[];
}

// askQuestions reads the questionnaire, dropping malformed entries.
// askQuestions 读取问卷，丢弃格式错误的条目。
export function askQuestions(value: unknown): AskQuestion[] {
  const questions: AskQuestion[] = [];
  for (const entry of listOf(value)) {
    const item = objectOf(entry);
    const header = textOf(item?.header);
    const question = textOf(item?.question);
    if (!header || !question) continue;
    const options = listOf(item?.options).map((option) => textOf(objectOf(option)?.label)).filter((label): label is string => Boolean(label));
    questions.push({ header, options, question });
  }
  return questions;
}

// UNANSWERED is the marker the host writes for a question left blank.
// UNANSWERED 是宿主为空白问题写入的标记。
const UNANSWERED = "Unanswered";

// askAnswers reads the answer to each question from the stable "## User Answers" reply; null entries are unanswered.
// Answers are matched by header in question order, so a header containing ": " still matches.
// askAnswers 从稳定的 "## User Answers" 回复读取每个问题的回答；null 表示未回答。按问题顺序以标题匹配，
// 因此包含 ": " 的标题仍能匹配。
export function askAnswers(text: string, questions: readonly AskQuestion[]): (string | null)[] | null {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  if (lines[0] !== "## User Answers") return null;
  const answers = lines.filter((line) => line.startsWith("- "));
  return questions.map((question, index) => {
    const prefix = `- ${question.header}: `;
    const line = answers[index]?.startsWith(prefix) ? answers[index] : answers.find((candidate) => candidate.startsWith(prefix));
    const answer = line?.slice(prefix.length) ?? null;
    return answer === null || answer === UNANSWERED ? null : answer;
  });
}

/** Renders one ask call.
 * 渲染一次 ask 调用。 */
export function AskCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const questions = React.useMemo(() => askQuestions(call.input.questions), [call.input.questions]);
  const answers = React.useMemo(
    () => (call.modelOutput && !call.isError ? askAnswers(call.modelOutput, questions) : null),
    [call.isError, call.modelOutput, questions],
  );
  const answered = answers?.filter((answer) => answer !== null).length ?? 0;
  const target = questions.length === 1 ? questions[0].question : questions.map((question) => question.header).join(" · ");
  return (
    <ToolFrame
      {...props}
      icon={MessageCircleQuestion}
      meta={answers ? <span>{t("toolCard.ask.answered").replace("{done}", String(answered)).replace("{total}", String(questions.length))}</span> : null}
      state={call.state}
      target={target}
      targetTitle={target}
      timed={false}
      verb={t("toolCard.ask.title")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : questions.length > 0 ? (
        <ol className="vk-qa">
          {questions.map((question, index) => {
            const answer = answers?.[index] ?? null;
            const picks = answer?.split(", ") ?? [];
            // A choice made only from the offered options is shown on the options themselves.
            // 只从提供选项中作出的选择直接标在选项上。
            const pickedOnly = answer !== null && picks.every((pick) => question.options.includes(pick));
            return (
              <li className="vk-qa-item" key={index}>
                <span className="vk-qa-head">{question.header}</span>
                <p className="vk-qa-question">{question.question}</p>
                {question.options.length > 0 ? (
                  <div className="vk-qa-options">
                    {question.options.map((option) => (
                      <span className="vk-badge" data-picked={picks.includes(option) ? "" : undefined} key={option}>{option}</span>
                    ))}
                  </div>
                ) : null}
                {pickedOnly ? null : (
                  <p className="vk-qa-answer" data-empty={answer === null ? "" : undefined}>
                    {answer ?? (answers ? t("toolCard.ask.unanswered") : t("toolCard.ask.waiting"))}
                  </p>
                )}
              </li>
            );
          })}
        </ol>
      ) : null}
    </ToolFrame>
  );
}
