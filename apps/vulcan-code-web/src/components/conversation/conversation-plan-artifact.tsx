import React from "react";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  PlayCircle,
  Send,
  X,
} from "lucide-react";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import { useI18n } from "@/i18n";

// PlanArtifactActionPayload describes one user decision for a pending Plan artifact.
// PlanArtifactActionPayload 描述用户对待确认计划产物的一次决策。
export interface PlanArtifactActionPayload {
  // Structured Plan artifact embedded in the transcript.
  // 嵌入转录中的结构化计划产物。
  artifact: ConversationPlanArtifact;
  // Optional user-authored note attached to the action.
  // 附加到操作上的可选用户输入说明。
  note?: string;
}

// PlanArtifactBlock renders a submitted Plan as a persistent transcript card.
// PlanArtifactBlock 将已提交计划渲染为常驻转录卡片。
export function PlanArtifactBlock(props: {
  // Submitted Plan artifact to render.
  // 需要渲染的已提交计划产物。
  artifact: ConversationPlanArtifact;
  // Whether this artifact is still waiting for a user decision.
  // 该计划产物是否仍在等待用户确认。
  pending: boolean;
  // Whether action buttons are temporarily disabled.
  // 操作按钮是否临时禁用。
  disabled: boolean;
  // Copies the Markdown source to the system clipboard.
  // 将 Markdown 源码复制到系统剪贴板。
  onCopyText: (text: string) => Promise<void> | void;
  // Executes the saved Plan as a normal build request.
  // 将已保存计划作为普通构建请求执行。
  onExecute: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion back into Plan mode for revision.
  // 将补充建议送回计划模式用于修订。
  onSuggest: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels the pending Plan without executing it.
  // 取消待确认计划且不执行。
  onCancel: (payload: PlanArtifactActionPayload) => Promise<void> | void;
}) {
  const { t } = useI18n();
  const artifactKey = `${props.artifact.path}:${props.artifact.updatedAt}`;
  const [expanded, setExpanded] = React.useState(props.pending);
  const [copied, setCopied] = React.useState(false);
  const [note, setNote] = React.useState("");
  const [selectedAction, setSelectedAction] = React.useState<"execute" | "defer">("execute");
  const confirmationRef = React.useRef<HTMLDivElement | null>(null);
  const noteInputRef = React.useRef<HTMLTextAreaElement | null>(null);
  const noteHasContent = Boolean(note.trim());

  React.useEffect(() => {
    setExpanded(props.pending);
    setNote("");
    setSelectedAction("execute");
  }, [artifactKey, props.pending]);

  React.useEffect(() => {
    if (props.pending) {
      window.requestAnimationFrame(() => noteInputRef.current?.focus());
    }
  }, [artifactKey, props.pending]);

  React.useEffect(() => {
    resizePlanNoteInput(noteInputRef.current);
  }, [note]);

  const handleCopy = React.useCallback(async () => {
    await props.onCopyText(props.artifact.content);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1100);
  }, [props]);

  const execute = React.useCallback(() => {
    void props.onExecute({ artifact: props.artifact, note: note.trim() || undefined });
  }, [note, props]);

  const defer = React.useCallback(() => {
    void props.onCancel({ artifact: props.artifact });
  }, [props]);

  const suggest = React.useCallback(() => {
    const trimmedNote = note.trim();
    if (!trimmedNote) {
      defer();
      return;
    }
    void props.onSuggest({ artifact: props.artifact, note: trimmedNote });
  }, [defer, note, props]);

  const submitSelectedAction = React.useCallback(() => {
    if (selectedAction === "defer") {
      suggest();
      return;
    }
    execute();
  }, [execute, selectedAction, suggest]);

  const handleConfirmationKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedAction((current) => (current === "execute" ? "defer" : "execute"));
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submitSelectedAction();
    }
  }, [submitSelectedAction]);

  return (
    <section
      className="plan-artifact-card"
      data-plan-artifact-pending={props.pending ? "true" : "false"}
      data-plan-artifact-expanded={expanded ? "true" : "false"}
    >
      <header className="plan-artifact-card-header">
        <div className="plan-artifact-card-title">
          <span>{props.artifact.name || t("planArtifact.titleFallback")}</span>
        </div>
        <div className="plan-artifact-card-actions">
          <button
            aria-label={copied ? t("planArtifact.copied") : t("planArtifact.copy")}
            className="plan-artifact-icon-button"
            onClick={() => void handleCopy()}
            title={copied ? t("planArtifact.copied") : t("planArtifact.copy")}
            type="button"
          >
            {copied ? <Check size={15} /> : <Copy size={15} />}
          </button>
          <button
            aria-expanded={expanded}
            aria-label={expanded ? t("planArtifact.collapse") : t("planArtifact.expand")}
            className="plan-artifact-icon-button"
            onClick={() => setExpanded((current) => !current)}
            title={expanded ? t("planArtifact.collapse") : t("planArtifact.expand")}
            type="button"
          >
            {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          </button>
        </div>
      </header>
      <div className="plan-artifact-card-body">
        <div className="plan-artifact-markdown-frame">
          <MarkdownMessage content={props.artifact.content} eager />
          {!expanded ? (
            <button className="plan-artifact-expand-overlay" onClick={() => setExpanded(true)} type="button">
              {t("planArtifact.expand")}
            </button>
          ) : null}
        </div>
      </div>
      {props.pending ? (
        <div
          ref={confirmationRef}
          aria-activedescendant={`plan-artifact-action-${selectedAction}`}
          aria-label={t("planArtifact.confirmAria")}
          className="plan-artifact-confirmation"
          onKeyDown={handleConfirmationKeyDown}
          tabIndex={0}
        >
          <textarea
            ref={noteInputRef}
            aria-label={t("planArtifact.noteAria")}
            className="plan-artifact-note-input"
            disabled={props.disabled}
            onChange={(event) => setNote(event.currentTarget.value)}
            onInput={(event) => resizePlanNoteInput(event.currentTarget)}
            placeholder={t("planArtifact.notePlaceholder")}
            rows={1}
            value={note}
          />
          <div className="plan-artifact-action-row">
            <button
              id="plan-artifact-action-execute"
              aria-pressed={selectedAction === "execute"}
              className="plan-artifact-action-choice plan-artifact-action-choice-primary"
              disabled={props.disabled}
              onClick={() => {
                setSelectedAction("execute");
                execute();
              }}
              type="button"
            >
              <PlayCircle size={13} />
              <span>{noteHasContent ? t("planArtifact.executeWithNote") : t("planArtifact.execute")}</span>
            </button>
            <button
              id="plan-artifact-action-defer"
              aria-pressed={selectedAction === "defer"}
              className="plan-artifact-action-choice"
              disabled={props.disabled}
              onClick={() => {
                setSelectedAction("defer");
                suggest();
              }}
              type="button"
            >
              {noteHasContent ? <Send size={13} /> : <X size={13} />}
              <span>{noteHasContent ? t("planArtifact.suggest") : t("planArtifact.defer")}</span>
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

// resizePlanNoteInput keeps the Plan confirmation input compact until the user writes multiple lines.
// resizePlanNoteInput 让计划确认输入框在用户输入多行前保持紧凑。
function resizePlanNoteInput(element: HTMLTextAreaElement | null): void {
  if (!element) {
    return;
  }
  element.style.height = "32px";
  element.style.height = `${Math.max(32, Math.min(element.scrollHeight + 2, 112))}px`;
}
