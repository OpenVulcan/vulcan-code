import React from "react";
import { metadataValue } from "@/components/conversation/conversation-metadata";
import { Badge } from "@/components/ui/badge";
import { useI18n } from "@/i18n";
import { dedicatedTurnForgotten, type DedicatedMemorySelection } from "@/service/dedicated-memory";

// DedicatedTranscriptMemoryContext carries the selected dedicated session's memory facts to transcript rows.
// DedicatedTranscriptMemoryContext 将选中专职会话的记忆事实传给转录行。
const DedicatedTranscriptMemoryContext = React.createContext<DedicatedMemorySelection | null>(null);

/** Provides dedicated memory facts once per transcript so rows never select them individually.
 * 每个转录只提供一次专职记忆事实，转录行无需各自选择。
 *
 * @param props.memory facts of the transcript's session; ordinary sessions provide null.
 *                     转录所属会话的事实；普通会话提供 null。 */
export function DedicatedTranscriptMemoryProvider(props: { memory: DedicatedMemorySelection | null; children: React.ReactNode }) {
  return (
    <DedicatedTranscriptMemoryContext.Provider value={props.memory}>
      {props.children}
    </DedicatedTranscriptMemoryContext.Provider>
  );
}

/** Reads one positive safe integer from presentation metadata.
 * 从展示元数据读取一个安全整数。 */
function metadataInteger(metadata: unknown, key: string): number | null {
  const value = metadataValue(metadata, [key]);
  return typeof value === "number" && Number.isSafeInteger(value) ? value : null;
}

/** Marks a user message as the base prompt or as part of a forgotten turn; renders nothing elsewhere.
 * 将用户消息标注为基础提示词或不记忆回合的一部分；其他情况不渲染。
 *
 * @param props.metadata V1 presentation metadata carrying `v1_seq` and `v1_turn_id`.
 *                       携带 `v1_seq` 与 `v1_turn_id` 的 V1 展示元数据。
 * @param props.body visible prompt text, compared with the immutable base prompt. */
export function DedicatedUserMessageMarks(props: { metadata: unknown; body: string }) {
  const { t } = useI18n();
  const memory = React.useContext(DedicatedTranscriptMemoryContext);
  const seq = metadataInteger(props.metadata, "v1_seq");
  if (memory === null || seq === null) return null;
  const turnId = metadataInteger(props.metadata, "v1_turn_id");
  const basePrompt = props.body.trim() === memory.basePrompt;
  const forgotten = dedicatedTurnForgotten(memory.lockedSpans, seq, turnId !== null && turnId === memory.openTurnId);
  if (!basePrompt && !forgotten) return null;
  return (
    <span className="dedicated-message-marks">
      {basePrompt ? (
        <Badge title={t("conversation.dedicated.basePromptHint")} variant="secondary">
          {t("conversation.dedicated.basePrompt")}
        </Badge>
      ) : null}
      {forgotten ? (
        <Badge title={t("conversation.dedicated.forgottenHint")} variant="outline">
          {t("conversation.dedicated.forgotten")}
        </Badge>
      ) : null}
    </span>
  );
}
