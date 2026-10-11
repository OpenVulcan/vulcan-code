import * as React from "react";
import { GitCompareArrows } from "lucide-react";

import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import type { TurnChangeSummaryFileV1, TurnChangeSummaryV1 } from "@/diff/turn-change-summary-v1";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import "./turn-change-file-list.css";

// TURN_CHANGE_INITIAL_FILES is how many files show before the list is expanded further.
// TURN_CHANGE_INITIAL_FILES 是列表进一步展开前显示的文件数量。
const TURN_CHANGE_INITIAL_FILES = 5;

// STATUS_LABEL_KEYS names the lifecycle tag shown beside non-modified files.
// STATUS_LABEL_KEYS 为非修改类文件旁显示的生命周期标签命名。
const STATUS_LABEL_KEYS: Partial<Record<TurnChangeSummaryFileV1["status"], TranslationKey>> = {
  added: "conversation.turnChanges.added",
  deleted: "conversation.turnChanges.deleted",
  renamed: "conversation.turnChanges.renamed",
};

/** Lists the files one Turn changed with their line totals.
 * 列出单个 Turn 改动的文件及其行数合计。
 *
 * Loaded on first expansion so its rules stay out of the eager stylesheet.
 * 首次展开时才加载，使其样式规则不进入首屏样式表。
 *
 * @param props.summary applied changes of the Turn.
 *                      该 Turn 的已应用变更。
 * @param props.onOpenInGit opens the review panel on these changes; absent when that panel shows another Turn.
 *                          打开展示这些变更的审核面板；该面板展示其他 Turn 时缺省。 */
export function TurnChangeFileList(props: {
  summary: TurnChangeSummaryV1;
  onOpenInGit?: () => void;
}) {
  const { locale, t } = useI18n();
  const [showAll, setShowAll] = React.useState(false);
  const files = showAll ? props.summary.files : props.summary.files.slice(0, TURN_CHANGE_INITIAL_FILES);
  const hiddenCount = props.summary.files.length - TURN_CHANGE_INITIAL_FILES;
  return (
    <section className="turn-change-files" aria-label={t("conversation.turnChanges.title")}>
      <header className="turn-change-files-header">
        <span>{t("conversation.turnChanges.title")}</span>
        {props.onOpenInGit ? (
          <button className="turn-change-files-action" onClick={props.onOpenInGit} type="button">
            <GitCompareArrows aria-hidden="true" size={13} />
            {t("conversation.turnChanges.openInGit")}
          </button>
        ) : null}
      </header>
      <ul>
        {files.map((file) => {
          const separator = file.path.lastIndexOf("/");
          const statusKey = STATUS_LABEL_KEYS[file.status];
          return (
            <li key={file.path} title={file.renamedFrom ? `${file.renamedFrom} → ${file.path}` : file.path}>
              <FileTypeIcon icon={resolveFileTypeIcon(file.path)} />
              <span className="turn-change-file-path">
                {separator >= 0 ? <span className="turn-change-file-dir">{file.path.slice(0, separator + 1)}</span> : null}
                {file.path.slice(separator + 1)}
              </span>
              {statusKey ? <span className={`turn-change-file-status ${file.status}`}>{t(statusKey)}</span> : null}
              <span className="file-change-added">+{file.additions.toLocaleString(locale)}</span>
              <span className="file-change-removed">-{file.deletions.toLocaleString(locale)}</span>
            </li>
          );
        })}
      </ul>
      {hiddenCount > 0 ? (
        <button className="turn-change-files-more" onClick={() => setShowAll((current) => !current)} type="button">
          {showAll
            ? t("conversation.turnChanges.showLess")
            : formatMessage(t("conversation.turnChanges.showMore"), { count: String(hiddenCount) })}
        </button>
      ) : null}
    </section>
  );
}
