import React from "react";
import { File, Folder, Puzzle, BookOpen, Server } from "lucide-react";

import { useI18n } from "@/i18n";
import { Badge } from "@/ui-kit/primitives/badge";

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
        // Tagged rows show their catalog title while retaining the exact inserted reference.
        // 带标签的条目显示目录名称，同时保留插入时的精确引用。
        const badge = entry.kind === "mcp" ? "MCP" : entry.kind === "luaSkill" ? "LuaSkills" : null;
        const name = entry.kind === "path"
          ? `${entry.entry.name}${entry.entry.isDirectory ? "/" : ""}`
          : badge ? entry.label : entry.displayName ?? entry.name;
        // Tagged rows show the introduction, and other shortcuts keep their existing secondary text.
        // 带标签的条目显示介绍，其余快捷指令保留现有的补充文字。
        const description = entry.kind === "path" ? null
          : badge ? entry.description?.trim() : entry.label === name ? null : entry.label;
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
              : entry.kind === "skill" ? <BookOpen aria-hidden="true" />
              : entry.kind === "mcp" ? <Server aria-hidden="true" /> : <Puzzle aria-hidden="true" />}
            <span className="vk-mention-names">
              <span className="vk-slash-cmd" title={entry.kind === "path" || badge ? name : entry.name}>{name}</span>
              {badge && <Badge variant="secondary">{badge}</Badge>}
              {description && <span className="vk-slash-desc" title={description}>{description}</span>}
            </span>
          </button>
        );
      })}
    </div>
  );
}
