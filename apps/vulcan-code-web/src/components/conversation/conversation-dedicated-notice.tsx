import { Lock, LockOpen } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";

/** Dedicated-session memory notice kinds persisted as display notes.
 * 以显示通知持久化的专职会话记忆通知种类。 */
export type DedicatedMemoryNoticeKind = "memory_locked" | "memory_unlocked";

/** Reports whether a display-note kind is a dedicated memory marker.
 * 判断显示通知种类是否为专职记忆标记。 */
export function isDedicatedMemoryNoticeKind(kind: string | null | undefined): kind is DedicatedMemoryNoticeKind {
  return kind === "memory_locked" || kind === "memory_unlocked";
}

/** Returns the localized marker title for one memory notice kind.
 * 返回单个记忆通知种类的本地化标记标题。 */
export function dedicatedMemoryNoticeTitle(
  kind: DedicatedMemoryNoticeKind,
  t: (key: TranslationKey) => string,
): string {
  return kind === "memory_locked"
    ? t("conversation.dedicatedNotice.locked")
    : t("conversation.dedicatedNotice.unlocked");
}

/** Renders a lock or unlock marker on the timeline; it is never assistant prose.
 * 在时间线上渲染锁定或解锁标记；它绝不是 assistant 正文。 */
export function DedicatedMemoryNotice(props: { kind: DedicatedMemoryNoticeKind; seq?: number }) {
  const { t } = useI18n();
  const Icon = props.kind === "memory_locked" ? Lock : LockOpen;
  return (
    <aside className="special-block notice dedicated-memory-marker" role="note" data-event-kind={props.kind} data-seq={props.seq}>
      <div className="special-block-main">
        <Icon size={16} aria-hidden="true" />
        <div>
          <strong>{dedicatedMemoryNoticeTitle(props.kind, t)}</strong>
          <small className="dedicated-memory-marker-hint">{props.kind === "memory_locked"
            ? t("conversation.dedicatedNotice.lockedHint")
            : t("conversation.dedicatedNotice.unlockedHint")}</small>
        </div>
      </div>
    </aside>
  );
}
