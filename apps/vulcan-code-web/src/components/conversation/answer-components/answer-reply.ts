import React from "react";

/** One reply the reader prepared on an answer component: the options picked in a `ui-decision`, or a comment on
 * any other component. `subject` names the commented component by its fence name and first line.
 * 读者在回答组件上准备的一条回复：`ui-decision` 中选取的选项，或对其他组件的批注。`subject` 以围栏名称与首行指明被批注的组件。 */
export type AnswerReplyEntry =
  | { kind: "decision"; question: string; picks: string[]; recommended: string[]; note: string }
  | { kind: "comment"; subject: string; note: string };

/** Decisions and comments the reader made in one conversation, in the order they were first made. ChatArea owns
 * the store so entries outlive transcript rows that unmount while scrolled away. Nothing is ever sent from here:
 * the composer takes the summarized text into its draft, where the reader edits and sends it.
 * 读者在一个会话中做出的决策与批注，按首次操作的顺序保存。存储由 ChatArea 持有，转录行滚出视口卸载后条目仍然保留。
 * 这里从不发送任何内容：输入区把汇总文本放入草稿，由读者编辑后自行发送。 */
export class AnswerReplyStore {
  private readonly entries = new Map<string, AnswerReplyEntry>();
  private readonly listeners = new Set<() => void>();
  private clears = 0;

  /** Subscribe in the shape `React.useSyncExternalStore` expects. 以 `React.useSyncExternalStore` 期望的形式订阅。 */
  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Number of prepared entries. 已准备的条目数。 */
  readonly size = () => this.entries.size;

  /** How many times the store was cleared; open editors close when it changes. 存储被清空的次数；变化时已打开的编辑框随之关闭。 */
  readonly generation = () => this.clears;

  /** Entry of one component, keyed by its fence. 单个组件的条目，以其围栏为键。 */
  get(key: string): AnswerReplyEntry | undefined {
    return this.entries.get(key);
  }

  /** All entries in first-made order. 按首次操作顺序排列的全部条目。 */
  list(): AnswerReplyEntry[] {
    return [...this.entries.values()];
  }

  /** Replace or, with null, remove one entry; an updated entry keeps its place. 替换条目，传 null 时移除；更新的条目保持原位。 */
  set(key: string, entry: AnswerReplyEntry | null): void {
    if (entry) this.entries.set(key, entry);
    else if (!this.entries.delete(key)) return;
    this.listeners.forEach((listener) => listener());
  }

  /** Remove every entry. 移除全部条目。 */
  clear(): void {
    if (this.entries.size === 0) return;
    this.entries.clear();
    this.clears += 1;
    this.listeners.forEach((listener) => listener());
  }
}

/** Present only inside a conversation whose composer can take the reply; elsewhere components stay read-only.
 * 仅在输入区可接收回复的会话中存在；其他位置的组件保持只读。 */
export const AnswerReplyContext = React.createContext<AnswerReplyStore | null>(null);
