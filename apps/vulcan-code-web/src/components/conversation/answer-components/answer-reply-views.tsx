import React from "react";
import { Check, MessageSquarePlus, SendHorizontal, TextCursorInput, X } from "lucide-react";
import { formatMessage, useI18n } from "@/i18n";
import type { AnswerComponent } from "./answer-component-model";
import { AnswerFrameActionContext } from "./answer-frame";
import { AnswerReplyContext, type AnswerReplyStore } from "./answer-reply";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { formatAnswerReply } from "./answer-reply-format";

// noStore keeps read-only surfaces on the same hook path as a live reply store.
// noStore 让只读界面与可用的回复存储走同一条 Hook 路径。
const noStore = () => () => {};

/** Entry of `replyKey` in the conversation's reply store; read-only surfaces get no store and no entry.
 * 会话回复存储中 `replyKey` 的条目；只读界面没有存储，也没有条目。 */
function useReplyEntry(replyKey: string | undefined) {
  const store = React.useContext(AnswerReplyContext);
  const live = store && replyKey !== undefined ? store : null;
  const entry = React.useSyncExternalStore(live?.subscribe ?? noStore, () => (live ? live.get(replyKey!) : undefined));
  return { live, entry };
}

/** Comment button shared by components and decisions. 组件与决策共用的批注按钮。 */
function CommentButton({ active, onClick }: { active: boolean; onClick: () => void }) {
  const { t } = useI18n();
  return (
    <IconButton aria-label={t("answerComponents.comment")} className="vk-codeblock-action" size="xs" aria-pressed={active}
      onClick={onClick} title={t("answerComponents.comment")} type="button">
      <MessageSquarePlus aria-hidden="true" />
    </IconButton>
  );
}

/** Note editor under a component; empty text removes the note. 组件下方的说明编辑框；文字清空即移除说明。 */
function AnswerNote({ focus, onChange, onClose, value }: { focus: number; onChange: (text: string) => void; onClose: () => void; value: string }) {
  const { t } = useI18n();
  const input = React.useRef<HTMLTextAreaElement | null>(null);
  React.useEffect(() => {
    if (focus > 0) input.current?.focus({ preventScroll: true });
  }, [focus]);
  return (
    <div className="vk-a-note">
      <textarea aria-label={t("answerComponents.comment")} onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !value.trim()) {
            event.preventDefault();
            onClose();
          }
        }}
        placeholder={t("answerComponents.commentPlaceholder")} ref={input} rows={2} value={value} />
      <IconButton aria-label={t("answerComponents.removeComment")} className="vk-codeblock-action" size="xs" onClick={() => {
        onChange("");
        onClose();
      }} title={t("answerComponents.removeComment")} type="button">
        <X aria-hidden="true" />
      </IconButton>
    </div>
  );
}

/** Reader's note state for one component: visible while open or nonempty; `focus` bumps to focus the editor. An
 * editor opened before the store was last cleared (its reply went to the composer) counts as closed.
 * 单个组件的读者说明状态：打开或非空时可见；`focus` 递增以聚焦编辑框。存储最近一次清空（回复已交给输入区）之前打开的
 * 编辑框视为已关闭。 */
function useNoteEditor(note: string, store: AnswerReplyStore | null) {
  const generation = React.useSyncExternalStore(store?.subscribe ?? noStore, () => store?.generation() ?? 0);
  const [focus, setFocus] = React.useState(0);
  const [openedAt, setOpenedAt] = React.useState<number | null>(null);
  return {
    focus,
    visible: openedAt === generation || note.trim() !== "",
    show: () => {
      setOpenedAt(generation);
      setFocus((value) => value + 1);
    },
    hide: () => setOpenedAt(null),
  };
}

/** Wrap a rendered component with the reader's comment: framed components show the button beside their own
 * actions, other components in a hover corner.
 * 为已渲染组件附加读者批注：框架型组件把按钮放在自身操作旁，其他组件放在悬停角落。 */
export function CommentableFence({ children, framed, replyKey, store, subject }: {
  children: React.ReactNode;
  framed: boolean;
  replyKey: string;
  store: AnswerReplyStore;
  subject: string;
}) {
  const entry = React.useSyncExternalStore(store.subscribe, () => store.get(replyKey));
  const note = entry?.kind === "comment" ? entry.note : "";
  const editor = useNoteEditor(note, store);
  const button = <CommentButton active={editor.visible} onClick={editor.show} />;
  return (
    <div className="vk-a-fence" data-hover-actions={framed ? undefined : "true"}>
      {framed ? <AnswerFrameActionContext.Provider value={button}>{children}</AnswerFrameActionContext.Provider> : (
        <>
          <span className="vk-figure-hover">{button}</span>
          {children}
        </>
      )}
      {editor.visible ? (
        <AnswerNote focus={editor.focus} onClose={editor.hide} value={note}
          onChange={(text) => store.set(replyKey, text.trim() ? { kind: "comment", subject, note: text } : null)} />
      ) : null}
    </div>
  );
}

/** A question with options the reader picks; picks and a note go to the reply store. Without a store (surfaces
 * that cannot reply) the options are shown read-only with the recommendation.
 * 读者从选项中作答的问题；所选项与说明写入回复存储。没有存储（无法回复的界面）时，选项以只读方式显示并标出建议项。 */
export function AnswerDecisionView({ decision, replyKey }: { decision: Extract<AnswerComponent, { kind: "decision" }>; replyKey?: string }) {
  const { t } = useI18n();
  const { live, entry } = useReplyEntry(replyKey);
  const state = entry?.kind === "decision" ? entry : null;
  const picks = state?.picks ?? [];
  const note = state?.note ?? "";
  const editor = useNoteEditor(note, live);
  const save = (nextPicks: string[], nextNote: string) => live?.set(replyKey!, nextPicks.length > 0 || nextNote.trim() ? {
    kind: "decision",
    // A decision without a question line is named in the reply by its options.
    // 没有问题行的决策在回复中以其选项命名。
    question: decision.question || decision.options.map((option) => option.label).join(" / "),
    picks: nextPicks,
    recommended: decision.options.filter((option) => option.recommended).map((option) => option.label),
    note: nextNote,
  } : null);
  const choose = (label: string) => {
    const picked = picks.includes(label);
    if (!decision.multiple) save(picked ? [] : [label], note);
    else save(decision.options.map((option) => option.label).filter((item) => item === label ? !picked : picks.includes(item)), note);
  };
  return (
    <section className="vk-figure vk-a-decision">
      <header className="vk-a-decision-head">
        <strong>{decision.question}</strong>
        {decision.multiple ? <small>{t("answerComponents.decision.multiple")}</small> : null}
        {live ? <CommentButton active={editor.visible} onClick={editor.show} /> : null}
      </header>
      <div aria-label={decision.question || undefined} className="vk-a-decision-options" role={decision.multiple ? "group" : "radiogroup"}>
        {decision.options.map((option) => {
          const picked = picks.includes(option.label);
          return (
            <button aria-checked={picked} className="vk-a-decision-option" data-multiple={decision.multiple || undefined}
              data-recommended={option.recommended || undefined} disabled={!live} key={option.label}
              onClick={() => choose(option.label)} role={decision.multiple ? "checkbox" : "radio"} type="button">
              <span aria-hidden="true" className="vk-a-decision-mark">{picked ? <Check size={11} strokeWidth={3} /> : null}</span>
              <span className="vk-a-decision-text">
                <span>{option.label}</span>
                {option.note ? <small>{option.note}</small> : null}
              </span>
              {option.recommended ? <span className="vk-badge" data-tone="ok">{t("answerComponents.decision.recommended")}</span> : null}
            </button>
          );
        })}
      </div>
      {live && editor.visible ? <AnswerNote focus={editor.focus} onChange={(text) => save(picks, text)} onClose={editor.hide} value={note} /> : null}
      {live && state ? <AnswerDecisionActions store={live} /> : null}
    </section>
  );
}

/** Actions under an answered decision: insert every prepared reply into the draft, or send them right away. Both
 * combine all decisions and comments in the store; they are shown only while a composer can take them.
 * 已作答决策下方的操作：把全部已准备的回复插入草稿，或立即发送。两者都合并存储中的全部决策与批注；仅在有输入区可接收时显示。 */
function AnswerDecisionActions({ store }: { store: AnswerReplyStore }) {
  const { t } = useI18n();
  const composer = React.useSyncExternalStore(store.subscribe, store.connectedComposer);
  const count = React.useSyncExternalStore(store.subscribe, store.size);
  if (!composer) return null;
  const deliver = (mode: "insert" | "send") => store.deliver(mode, (entries) => formatAnswerReply(entries, t));
  return (
    <footer className="vk-a-decision-actions">
      {count > 1 ? <small>{formatMessage(t("answerComponents.decision.combined"), { count: String(count) })}</small> : null}
      <Button onClick={() => deliver("insert")} size="sm" type="button" variant="outline">
        <TextCursorInput aria-hidden="true" size={14} />{t("answerComponents.decision.insert")}
      </Button>
      <Button onClick={() => deliver("send")} size="sm" type="button">
        <SendHorizontal aria-hidden="true" size={14} />{t("answerComponents.decision.send")}
      </Button>
    </footer>
  );
}
