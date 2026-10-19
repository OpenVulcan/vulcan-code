// Unified diff of one change-set file, drawn with the kit diff rows.
// 以套件差异行绘制的单个变更集文件统一差异视图。
import { useI18n } from "@/i18n";
import type { PersistentChangeSetFile } from "@/service/conversation-event-window-store";
import { fileDiffRows } from "./change-set";

// DIFF_MARKS gives each row kind its gutter sign.
// DIFF_MARKS 为每种行类型指定边栏符号。
const DIFF_MARKS = { context: "", insert: "+", delete: "−", omit: "⋯" } as const;

/** Renders every row of one file; omit rows say how much was elided.
 * 渲染单个文件的所有行；省略行说明省略了多少。 */
export function DiffView(props: { file: PersistentChangeSetFile }) {
  const { t } = useI18n();
  const rows = fileDiffRows(props.file);
  if (rows.length === 0) return <div className="vk-tool-note">{t("toolCard.diff.empty")}</div>;
  return (
    <div className="vk-diff" role="table">
      {rows.map((row, index) => (
        <div className="vk-diff-row" data-kind={row.kind} key={index} role="row">
          <span>{row.oldLine ?? ""}</span>
          <span>{row.newLine ?? ""}</span>
          <span>{DIFF_MARKS[row.kind]}</span>
          <span>{row.kind === "omit"
            ? row.omitted === undefined
              ? t("toolDomain.changeSet.omittedByProducer")
              : t("toolDomain.changeSet.omittedLines").replace("{count}", String(row.omitted))
            : row.text || " "}</span>
        </div>
      ))}
    </div>
  );
}

/** Renders the five-cell added/removed proportion bar beside a +/− count.
 * 在 +/− 计数旁渲染五格新增/删除比例条。 */
export function DiffStat(props: { added: number; removed: number }) {
  // Each non-zero side keeps at least one cell so a one-line change still shows its color.
  // 每个非零侧至少保留一格，使单行变更也能显示其颜色。
  const total = props.added + props.removed;
  const addedCells = props.added === 0 ? 0 : Math.min(props.removed > 0 ? 4 : 5, Math.max(1, Math.round((props.added / total) * 5)));
  const removedCells = props.removed > 0 ? 5 - addedCells : 0;
  return (
    <>
      <span className="vk-add">+{props.added}</span>
      <span className="vk-del">−{props.removed}</span>
      <span aria-hidden="true" className="vk-diffstat">
        {Array.from({ length: 5 }, (_, index) => (
          <i data-k={index < addedCells ? "a" : index < addedCells + removedCells ? "d" : undefined} key={index} />
        ))}
      </span>
    </>
  );
}
