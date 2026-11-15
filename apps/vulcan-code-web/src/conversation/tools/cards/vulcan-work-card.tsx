import { Fragment } from "react";
import { FileText, Presentation, Sheet, TextSearch } from "lucide-react";
import { useI18n } from "@/i18n";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { objectOf, listOf, textOf, toolDiagnosticText } from "../tool-call";
import { JsonView } from "../views/json-view";
import { MarkdownResult, ToolDiagnostic } from "../views/result-views";
import { vulcanWorkOperation, vulcanWorkPresentation } from "./vulcan-work-model";

// FORMAT_ICONS distinguish the four supported formats in the shared card frame.
// FORMAT_ICONS 在共享工具卡中区分四种支持的格式。
const FORMAT_ICONS = { pdf: FileText, xlsx: Sheet, docx: TextSearch, pptx: Presentation };

/** Renders a known Vulcan Work operation with file targets, result facts and tool diagnostics.
 * 渲染已知的 Vulcan Work 操作及其文件目标、结果事实和工具诊断。 */
export function VulcanWorkCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const operation = vulcanWorkOperation(call);
  if (!operation) return null;

  // The caption uses the basename while retaining the exact path as a tooltip.
  // 标题使用文件名，悬停时保留准确路径。
  const work = vulcanWorkPresentation(call, operation);
  const target = work.targetPath?.split(/[\\/]/).at(-1) ?? "";
  const diagnostics = toolDiagnosticText(call);
  const hasArguments = Object.keys(work.input).length > 0;
  // Sheet and range are sourced from the requested scope, so they appear before completion too.
  // 工作表和区域来自请求范围，因此在执行完成之前也可显示。
  const range = [work.sheet, work.range].filter(Boolean).join(" · ");
  const paths = [
    { key: "source", path: operation.family === "edit" ? work.sourcePath : null },
    { key: "output", path: work.outputPath },
  ] as const;
  return (
    <ToolFrame
      {...props}
      icon={FORMAT_ICONS[operation.format]}
      state={call.state}
      verb={t(`toolCard.work.${operation.id}`)}
      target={target}
      targetTitle={work.targetPath ?? undefined}
      filePath={work.filePath}
      meta={(
        <>
          {range ? <span>{range}</span> : null}
          {work.counts.map(({ key, count }) => (
            <span key={key}>{t(`toolCard.work.${key}`)} {count}</span>
          ))}
        </>
      )}
    >
      {hasArguments || work.result || call.modelOutput || diagnostics ? (
        <>
          {paths.some(({ path }) => path) ? (
            <dl className="vk-facts">
              {paths.map(({ key, path }) => path ? (
                <Fragment key={key}>
                  <dt>{t(`toolCard.work.${key}`)}</dt>
                  <dd title={path}>{path}</dd>
                </Fragment>
              ) : null)}
            </dl>
          ) : null}
          {work.result ? <WorkResult result={work.result} /> : (
            call.modelOutput ? <MarkdownResult error={call.isError} text={call.modelOutput} /> : null
          )}
          {diagnostics ? <ToolDiagnostic label={t("toolCard.generic.diagnostic")} text={diagnostics} /> : null}
          {hasArguments ? (
            <details className="vk-model-more">
              <summary>{t("toolCard.generic.input")}</summary>
              <JsonView value={work.input} />
            </details>
          ) : null}
        </>
      ) : null}
    </ToolFrame>
  );
}

/** Shows worksheet names and their visibility, with the complete result available for inspection.
 * 直接显示工作表名称与可见状态，并提供完整结果供检查。 */
function WorkResult(props: { result: Record<string, unknown> }) {
  const { t } = useI18n();
  // Worksheet visibility comes directly from the workbook listing contract.
  // 工作表可见性直接来自工作簿列表契约。
  const sheets = listOf(props.result.sheets).flatMap((item) => {
    const sheet = objectOf(item);
    const name = textOf(sheet?.name);
    const visibility = sheet?.visibility;
    return name && (visibility === "visible" || visibility === "hidden" || visibility === "veryHidden")
      ? [{ name, visibility }] as const : [];
  });
  return (
    <>
      {sheets.length > 0 ? (
        <dl className="vk-facts">
          {sheets.map(({ name, visibility }) => (
            <Fragment key={name}>
              <dt>{name}</dt>
              <dd>{t(`toolCard.work.sheet.${visibility}`)}</dd>
            </Fragment>
          ))}
        </dl>
      ) : null}
      <details className="vk-model-more">
        <summary>{t("toolCard.work.result")}</summary>
        <JsonView value={props.result} />
      </details>
    </>
  );
}
