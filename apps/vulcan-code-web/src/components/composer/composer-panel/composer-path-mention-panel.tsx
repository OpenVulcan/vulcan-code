import React from "react";
import { File, Folder } from "lucide-react";

import { useI18n } from "@/i18n";

import type { ComposerPathMention } from "./use-composer-path-mention";

// ComposerPathMentionPanel lists the workspace entries offered for the @ word at the caret, in the slash panel's
// style. Folders end with a slash because picking one steps into it.
// ComposerPathMentionPanel 以 slash 面板的样式列出为光标处 @ 词提供的工作区条目。文件夹以斜杠结尾，因为选择它会进入其中。
export function ComposerPathMentionPanel(props: { mention: ComposerPathMention }) {
  const { t } = useI18n();
  const { suggestions, highlightedIndex, accept, setHighlightedIndex } = props.mention;
  const listRef = React.useRef<HTMLDivElement | null>(null);
  // The highlighted row stays in view while the arrows move past the panel's visible rows.
  // 方向键移过面板可见行时，高亮行保持在视野内。
  React.useLayoutEffect(() => {
    listRef.current?.children[highlightedIndex]?.scrollIntoView?.({ block: "nearest" });
  }, [highlightedIndex]);
  return (
    <div aria-label={t("composer.mention.panel")} className="vk-slash" data-kind="path" ref={listRef} role="listbox">
      {suggestions.map((entry, index) => (
        <button
          aria-selected={index === highlightedIndex}
          className="vk-slash-row"
          key={entry.relativePath}
          // The textarea keeps focus, so the caret and the @ word stay where they are.
          // 文本框保持焦点，使光标与 @ 词保持原位。
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => accept(entry)}
          onMouseEnter={() => setHighlightedIndex(index)}
          role="option"
          tabIndex={-1}
          type="button"
        >
          {entry.isDirectory ? <Folder aria-hidden="true" /> : <File aria-hidden="true" />}
          <span className="vk-slash-cmd">{entry.isDirectory ? `${entry.name}/` : entry.name}</span>
        </button>
      ))}
    </div>
  );
}
