import React from "react";

import { DEFAULT_WORKSPACE_FILE_FILTERS, loadWorkspaceFileDirectory } from "@/service/workbench-snapshot-service";
import type { WorkspaceFileEntrySummary } from "@/types";

// Path mention: typing @ at the start of a word lists the project's files and folders, one folder level at a time,
// the way a shell completes a path. Picking a folder steps into it; picking a file writes its workspace-relative
// path after the @, which the model reads like any other file path in the prompt.
// 路径提及：在词首输入 @ 会列出项目的文件与文件夹，像 shell 补全路径那样逐级展开。选择文件夹会进入该文件夹；
// 选择文件会在 @ 后写入其工作区相对路径，模型会像读取提示词中的其他文件路径一样读取它。

// PATH_MENTION_LIMIT bounds the rows the panel offers at once.
// PATH_MENTION_LIMIT 限制面板一次提供的行数。
export const PATH_MENTION_LIMIT = 50;

// PathMention is the @ word around the caret: where it starts and ends in the draft, and what is typed before the caret.
// PathMention 是光标所在的 @ 词：它在草稿中的起止位置，以及光标前已输入的内容。
export interface PathMention {
  start: number;
  end: number;
  query: string;
}

// pathMentionAt finds the @ word the caret is in; the @ must start the word so addresses like a@b are left alone.
// pathMentionAt 查找光标所在的 @ 词；@ 必须位于词首，因此 a@b 这类地址不受影响。
export function pathMentionAt(input: string, caret: number): PathMention | null {
  let start = caret;
  while (start > 0 && !/\s/u.test(input[start - 1])) start -= 1;
  if (input[start] !== "@") return null;
  let end = caret;
  while (end < input.length && !/\s/u.test(input[end])) end += 1;
  return { start, end, query: input.slice(start + 1, caret) };
}

// splitPathMention separates the folder already typed from the name being typed in it; both slash styles count.
// splitPathMention 将已输入的文件夹与其中正在输入的名称分开；两种斜杠都适用。
export function splitPathMention(query: string): { directory: string; prefix: string } {
  const slash = Math.max(query.lastIndexOf("/"), query.lastIndexOf("\\"));
  return slash < 0
    ? { directory: "", prefix: query }
    : { directory: query.slice(0, slash).replace(/\\/gu, "/"), prefix: query.slice(slash + 1) };
}

// pathMentionSuggestions lists the folder's entries whose name starts with the typed text, then those that only
// contain it, folders first in each group.
// pathMentionSuggestions 列出名称以已输入文本开头的条目，再列出仅包含该文本的条目，每组中文件夹在前。
export function pathMentionSuggestions(entries: readonly WorkspaceFileEntrySummary[], prefix: string): WorkspaceFileEntrySummary[] {
  const needle = prefix.toLowerCase();
  const byKind = (a: WorkspaceFileEntrySummary, b: WorkspaceFileEntrySummary) => Number(b.isDirectory) - Number(a.isDirectory) || a.name.localeCompare(b.name);
  const starts = entries.filter((entry) => entry.name.toLowerCase().startsWith(needle)).sort(byKind);
  const contains = needle ? entries.filter((entry) => !entry.name.toLowerCase().startsWith(needle) && entry.name.toLowerCase().includes(needle)).sort(byKind) : [];
  return [...starts, ...contains].slice(0, PATH_MENTION_LIMIT);
}

// ComposerPathMention is the panel state and the keyboard and pointer actions the composer wires in.
// ComposerPathMention 是输入区接入的面板状态及键盘与指针操作。
export interface ComposerPathMention {
  suggestions: WorkspaceFileEntrySummary[];
  visible: boolean;
  highlightedIndex: number;
  setHighlightedIndex: (index: number) => void;
  accept: (entry: WorkspaceFileEntrySummary) => void;
  // trackCaret records where the caret is after typing, clicking or moving it.
  // trackCaret 记录输入、点击或移动后的光标位置。
  trackCaret: (textarea: HTMLTextAreaElement) => void;
  // handleKeyDown reports whether the open panel consumed the key.
  // handleKeyDown 报告打开的面板是否消费了该按键。
  handleKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => boolean;
}

// useComposerPathMention offers workspace paths for the @ word at the caret.
// useComposerPathMention 为光标处的 @ 词提供工作区路径。
export function useComposerPathMention(options: {
  input: string;
  setInput: (value: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  // workspacePath is the project folder; without one there is nothing to offer.
  // workspacePath 是项目文件夹；没有时不提供任何内容。
  workspacePath: string | null;
  projectId: string | null;
  conversationId: string | null;
}): ComposerPathMention {
  const { input, setInput, textareaRef, workspacePath, projectId, conversationId } = options;
  const [caret, setCaret] = React.useState<number | null>(null);
  const [highlightedIndex, setHighlightedIndex] = React.useState(0);
  // dismissed is the mention Escape closed, kept until the mention changes.
  // dismissed 是被 Escape 关闭的提及，保留到提及内容变化为止。
  const [dismissed, setDismissed] = React.useState<string | null>(null);
  // listings caches each folder's entries for the current workspace; a failed folder stays empty.
  // listings 缓存当前工作区每个文件夹的条目；读取失败的文件夹保持为空。
  const [listings, setListings] = React.useState<{ workspacePath: string | null; folders: ReadonlyMap<string, readonly WorkspaceFileEntrySummary[] | null> }>(
    { workspacePath, folders: new Map() },
  );
  const folders = listings.workspacePath === workspacePath ? listings.folders : null;

  const mention = workspacePath && caret !== null && caret <= input.length ? pathMentionAt(input, caret) : null;
  const split = mention ? splitPathMention(mention.query) : null;
  const directory = split?.directory ?? null;

  React.useEffect(() => {
    if (!workspacePath || directory === null || folders?.has(directory)) return;
    const store = (entries: readonly WorkspaceFileEntrySummary[] | null) => setListings((current) => {
      const next = new Map(current.workspacePath === workspacePath ? current.folders : []);
      next.set(directory, entries);
      return { workspacePath, folders: next };
    });
    // A null marks the folder as requested, so typing inside it does not ask again.
    // null 表示该文件夹已请求过，因此在其中继续输入不会重复请求。
    store(null);
    loadWorkspaceFileDirectory(workspacePath, projectId, conversationId, directory || null, DEFAULT_WORKSPACE_FILE_FILTERS)
      .then((listing) => store(listing.entries))
      .catch(() => store([]));
  }, [conversationId, directory, folders, projectId, workspacePath]);

  const entries = directory === null ? null : folders?.get(directory) ?? null;
  const suggestions = React.useMemo(
    () => (entries && split ? pathMentionSuggestions(entries, split.prefix) : []),
    [entries, split?.prefix],
  );
  const mentionKey = mention ? `${mention.start}:${mention.query}` : null;
  const visible = suggestions.length > 0 && mentionKey !== dismissed;
  const highlighted = Math.min(highlightedIndex, Math.max(suggestions.length - 1, 0));

  React.useEffect(() => {
    setHighlightedIndex(0);
  }, [mentionKey]);

  const accept = React.useCallback((entry: WorkspaceFileEntrySummary) => {
    if (!mention) return;
    const path = entry.relativePath.replace(/\\/gu, "/");
    const replacement = `@${path}${entry.isDirectory ? "/" : " "}`;
    const next = input.slice(0, mention.start) + replacement + input.slice(mention.end).replace(/^\s/u, entry.isDirectory ? "$&" : "");
    const position = mention.start + replacement.length;
    setInput(next);
    setCaret(position);
    window.requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      if (!textarea) return;
      textarea.focus();
      textarea.setSelectionRange(position, position);
    });
  }, [input, mention, setInput, textareaRef]);

  const trackCaret = React.useCallback((textarea: HTMLTextAreaElement) => {
    setCaret(textarea.selectionStart === textarea.selectionEnd ? textarea.selectionStart : null);
  }, []);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!visible || event.nativeEvent.isComposing || event.altKey || event.ctrlKey || event.metaKey) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setHighlightedIndex((highlighted + step + suggestions.length) % suggestions.length);
      return true;
    }
    if ((event.key === "Enter" && !event.shiftKey) || (event.key === "Tab" && !event.shiftKey)) {
      event.preventDefault();
      accept(suggestions[highlighted]);
      return true;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setDismissed(mentionKey);
      return true;
    }
    return false;
  };

  return { suggestions, visible, highlightedIndex: highlighted, setHighlightedIndex, accept, trackCaret, handleKeyDown };
}
