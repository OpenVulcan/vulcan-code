import React from "react";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { ANSWER_CODE_MAX_EXCERPTS } from "@/generated/protocol-limits";
import { formatMessage, useI18n } from "@/i18n";
import type { AnswerCodeUnavailableReason } from "@/service/conversation-event-window-store";
import type { AnswerCodeExcerpt } from "./answer-component-model";
import type { AnswerComponentHost } from "./answer-component-view";

// `ui-code` excerpts. The lines come from the capture the host stored when the reply was sealed, never from the
// file's current content, so the reply keeps showing the code it was written against.
// `ui-code` 摘录。代码行来自宿主在回答封口时存储的抓取结果，而非文件当前内容，使回答保持显示其写作当时的代码。

// Catalog keys of the reasons a capture is unavailable.
// 抓取结果不可用原因的目录键。
const REASON_KEYS = {
  not_found: "answerComponents.code.reason.not_found",
  no_workspace: "answerComponents.code.reason.no_workspace",
  beyond_end: "answerComponents.code.reason.beyond_end",
  not_text: "answerComponents.code.reason.not_text",
  too_large: "answerComponents.code.reason.too_large",
  read_failed: "answerComponents.code.reason.read_failed",
} as const satisfies Record<AnswerCodeUnavailableReason, string>;

/** Render every excerpt of one `ui-code` fence.
 * 渲染一个 `ui-code` 围栏的全部摘录。 */
export function AnswerCodeView({ excerpts, host }: { excerpts: AnswerCodeExcerpt[]; host: AnswerComponentHost }) {
  return (
    <div className="answer-code">
      {excerpts.map((excerpt, index) => <AnswerCodeExcerptView excerpt={excerpt} host={host} key={`${index}:${excerpt.location}`} />)}
    </div>
  );
}

// One excerpt: its captured lines with line numbers, or a figure that says why they are missing.
// 单个摘录：带行号的已抓取行，或说明缺失原因的框。
function AnswerCodeExcerptView({ excerpt, host }: { excerpt: AnswerCodeExcerpt; host: AnswerComponentHost }) {
  const { t } = useI18n();
  const state = host.codeSnapshots;
  const snapshot = state && !state.pending ? state.snapshots.find((item) => item.location === excerpt.location) : undefined;
  const text = snapshot?.result.status === "captured" ? snapshot.result.text : null;
  const lastLine = text === null ? excerpt.endLine : excerpt.startLine + text.split("\n").length - 1;
  const highlighted = React.useMemo(() => new Set(excerpt.highlighted), [excerpt.highlighted]);
  const icon = React.useMemo(() => resolveFileTypeIcon(excerpt.path), [excerpt.path]);
  const href = host.fileHref(excerpt.path);
  const name = excerpt.path.split(/[\\/]/u).filter(Boolean).pop() ?? excerpt.path;
  const range = lastLine === excerpt.startLine ? `${excerpt.startLine}` : `${excerpt.startLine}–${lastLine}`;
  const header = (
    <span className="answer-code-title" title={text === null ? excerpt.path : `${excerpt.path} · ${t("answerComponents.code.snapshot")}`}>
      <FileTypeIcon icon={icon} />
      {href ? <a href={href} rel="noreferrer" target="_blank">{name}</a> : <span>{name}</span>}
      <span className="answer-code-range">{range}</span>
      {excerpt.note ? <span className="answer-code-note">{excerpt.note}</span> : null}
    </span>
  );
  if (text !== null) {
    return <>{host.renderCode({ code: text, path: excerpt.path, header, firstLine: excerpt.startLine, highlighted })}</>;
  }
  // A sealed reply missing this location either reached the excerpt limit or was sealed without captures.
  // 已封口回答缺少该位置，要么达到摘录上限，要么封口时没有抓取。
  const message = state?.pending
    ? t("answerComponents.code.pending")
    : snapshot?.result.status === "unavailable"
      ? t(REASON_KEYS[snapshot.result.reason])
      : state && state.snapshots.length >= ANSWER_CODE_MAX_EXCERPTS
        ? formatMessage(t("answerComponents.code.limit"), { count: ANSWER_CODE_MAX_EXCERPTS })
        : t("answerComponents.code.notCaptured");
  return (
    <figure className="markdown-code-block answer-code-missing" data-pending={state?.pending || undefined}>
      <figcaption className="markdown-code-header">{header}</figcaption>
      <p>{message}</p>
    </figure>
  );
}
