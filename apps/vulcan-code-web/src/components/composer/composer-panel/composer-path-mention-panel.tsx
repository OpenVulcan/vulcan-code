import React from "react";
import { File, Folder, Puzzle, BookOpen } from "lucide-react";

import { useI18n } from "@/i18n";

import type { ComposerPathMention } from "./use-composer-path-mention";
import { composerListOptionId } from "./use-composer-textarea-keys";

// ComposerPathMentionPanel lists the workspace entries offered for the @ word at the caret, in the slash panel's
// style. Folders end with a slash because picking one steps into it.
// ComposerPathMentionPanel 以 slash 面板的样式列出为光标处 @ 词提供的工作区条目。文件夹以斜杠结尾，因为选择它会进入其中。
export function ComposerPathMentionPanel(props: { mention: ComposerPathMention; id: string }) {
  const { t } = useI18n();
  const { suggestions, highlightedIndex, accept, setHighlightedIndex } = props.mention;
  const listRef = React.useRef<HTMLDivElement | null>(null);
  // The highlighted row stays in view while the arrows move past the panel's visible rows.
  // 方向键移过面板可见行时，高亮行保持在视野内。
  React.useLayoutEffect(() => {
    listRef.current?.children[highlightedIndex]?.scrollIntoView?.({ block: "nearest" });
  }, [highlightedIndex]);
  return (
    <div aria-label={t(props.mention.marker === "#" ? "area.skills" : "composer.mention.references")} className="vk-slash" data-kind={props.mention.marker === "#" ? "plugin" : "path"} id={props.id} ref={listRef} role="listbox">
      {suggestions.map((entry, index) => {
        // Keep the visible identifier short for bundled guides while retaining the exact reference.
        // 随包指南显示简短标识，同时保留精确的引用名称。
        const name = entry.kind === "path"
          ? `${entry.entry.name}${entry.entry.isDirectory ? "/" : ""}`
          : entry.displayName ?? entry.name;
        return (
          <button
            aria-selected={index === highlightedIndex}
            className="vk-slash-row"
            id={composerListOptionId(props.id, index)}
            key={entry.kind === "path" ? `path:${entry.entry.relativePath}` : `${entry.kind}:${entry.name}`}
            // The textarea keeps focus, so the caret and the @ word stay where they are.
            // 文本框保持焦点，使光标与 @ 词保持原位。
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => accept(entry)}
            onMouseEnter={() => setHighlightedIndex(index)}
            role="option"
            tabIndex={-1}
            type="button"
          >
            {entry.kind === "path"
              ? entry.entry.isDirectory ? <Folder aria-hidden="true" /> : <File aria-hidden="true" />
              : entry.kind === "skill" ? <BookOpen aria-hidden="true" /> : <Puzzle aria-hidden="true" />}
            <span className="vk-mention-names">
              <span className="vk-slash-cmd" title={entry.kind === "path" ? name : entry.name}>{name}</span>
              {entry.kind !== "path" && entry.label !== name ? (
                <span className="vk-slash-desc" title={entry.label}>{entry.label}</span>
              ) : null}
            </span>
          </button>
        );
      })}
    </div>
  );
}
