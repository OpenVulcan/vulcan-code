import React from "react";

/** One reply the reader prepared on an answer component: the options picked in a `ui-decision`, or a comment on
 * any other component. `subject` names the commented component by its fence name and first line.
 * 读者在回答组件上准备的一条回复：`ui-decision` 中选取的选项，或对其他组件的批注。`subject` 以围栏名称与首行指明被批注的组件。 */
export type AnswerReplyEntry =
  | { kind: "decision"; question: string; picks: string[]; recommended: string[]; note: string }
  | { kind: "comment"; subject: string; note: string };

/** The composer side of a conversation's replies: `insert` appends text to the draft, `send` appends it and sends
 * the draft through the composer's own send path.
 * 会话回复的输入区一侧：`insert` 把文字追加到草稿，`send` 追加后经输入区自身的发送路径发送草稿。 */
export interface AnswerReplyComposer {
  insert: (text: string) => void;
  send: (text: string) => void;
}

/** Decisions and comments the reader made in one conversation, in the order they were first made. ChatArea owns
 * the store so entries outlive transcript rows that unmount while scrolled away. The store never sends on its own:
 * only a reader's click hands the summarized text to the connected composer, which inserts it into the draft or
 * sends the draft through its normal send path.
 * 读者在一个会话中做出的决策与批注，按首次操作的顺序保存。存储由 ChatArea 持有，转录行滚出视口卸载后条目仍然保留。
 * 存储从不自行发送：只有读者点击后，才把汇总文本交给已连接的输入区，由其插入草稿或经正常发送路径发送草稿。 */
export class AnswerReplyStore {
  private readonly entries = new Map<string, AnswerReplyEntry>();
  private readonly listeners = new Set<() => void>();
  private clears = 0;
  private composer: AnswerReplyComposer | null = null;

  /** The composer that takes replies, or null while none can (no composer, or its draft is locked).
   * 接收回复的输入区；没有输入区或其草稿被锁定时为 null。 */
  readonly connectedComposer = () => this.composer;

  /** Register the composer that takes replies; returns the disconnect. 登记接收回复的输入区；返回断开函数。 */
  connectComposer(composer: AnswerReplyComposer): () => void {
    this.composer = composer;
    this.listeners.forEach((listener) => listener());
    return () => {
      if (this.composer !== composer) return;
      this.composer = null;
      this.listeners.forEach((listener) => listener());
    };
  }

  /** Hand every entry, formatted by `format`, to the composer to insert into the draft or to send, then clear the
   * store. Returns false and keeps the entries when no composer is connected or nothing is prepared.
   * 将全部条目经 `format` 格式化后交给输入区插入草稿或直接发送，然后清空存储。没有已连接的输入区或没有条目时
   * 返回 false 并保留条目。 */
  deliver(mode: "insert" | "send", format: (entries: AnswerReplyEntry[]) => string): boolean {
    const composer = this.composer;
    if (!composer || this.entries.size === 0) return false;
    const text = format(this.list());
    this.clear();
    if (mode === "send") composer.send(text);
    else composer.insert(text);
    return true;
  }

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
