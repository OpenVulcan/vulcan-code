import React from "react";
import {
  Check,
  ChevronDown,
  ClipboardList,
  Copy,
  PlayCircle,
  Send,
  X,
} from "lucide-react";
import { Button, IconButton } from "@/ui-kit/primitives/button";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import { useI18n } from "@/i18n";
import { runConversationControlAction } from "./conversation-control-feedback";

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
  const [expanded, setExpanded] = useConversationMeasuredExpansion(`plan-artifact:${artifactKey}`, props.pending);
  const collapsedPreviewHeight = usePlanArtifactCollapsedPreviewHeight();
  const [copied, setCopied] = React.useState(false);
  // actionInFlight closes the synchronous gap before the parent publishes its busy state.
  // actionInFlight 关闭父组件发布忙碌状态前的同步空隙。
  const actionInFlight = React.useRef(false);
  // actionPending keeps every input and keyboard path consistent during one submitted decision.
  // actionPending 在一次决策提交期间保持输入与键盘路径一致。
  const [actionPending, setActionPending] = React.useState(false);
  // actionsDisabled combines authoritative runtime state with the local in-flight decision.
  // actionsDisabled 合并权威运行时状态与本地进行中的决策。
  const actionsDisabled = props.disabled || actionPending;
  const [note, setNote] = React.useState("");
  const [selectedAction, setSelectedAction] = React.useState<"execute" | "defer">("execute");
  const confirmationRef = React.useRef<HTMLDivElement | null>(null);
  const noteInputRef = React.useRef<HTMLTextAreaElement | null>(null);
  const noteHasContent = Boolean(note.trim());

  React.useEffect(() => {
    setExpanded(props.pending, { manual: false });
    setNote("");
    setSelectedAction("execute");
  }, [artifactKey, props.pending, setExpanded]);

  React.useEffect(() => {
    if (props.pending) {
      window.requestAnimationFrame(() => noteInputRef.current?.focus());
    }
  }, [artifactKey, props.pending]);

  React.useEffect(() => {
    resizePlanNoteInput(noteInputRef.current);
  }, [note]);

  const handleCopy = React.useCallback(async () => {
    setCopied(false);
    if (await runConversationControlAction(
      () => props.onCopyText(props.artifact.content), t("conversation.control.copyFailed"),
    )) setCopied(true);
  }, [props, t]);

  React.useEffect(() => {
    if (!copied) return;
    // timer expires the clipboard confirmation and is released on unmount.
    // timer 使复制确认到期，并在卸载时释放。
    const timer = window.setTimeout(() => setCopied(false), 1100);
    return () => window.clearTimeout(timer);
  }, [copied]);

  /** Submits one exact plan decision, rejecting disabled and duplicate gestures.
   * 提交一次精确计划决策，拒绝禁用状态与重复手势。
   * action is the existing plan callback; completion releases the local UI lock.
   * action 是既有计划回调；结束后释放本地界面锁。 */
  const runDecision = React.useCallback(async (action: () => Promise<void> | void) => {
    if (!props.pending || props.disabled || actionInFlight.current) return;
    actionInFlight.current = true;
    setActionPending(true);
    try {
      await runConversationControlAction(action, t("conversation.control.actionFailed"));
    } finally {
      actionInFlight.current = false;
      setActionPending(false);
    }
  }, [props.pending, props.disabled, t]);

  const execute = React.useCallback(() => {
    void runDecision(() => props.onExecute({ artifact: props.artifact, note: note.trim() || undefined }));
  }, [note, props, runDecision]);

  const defer = React.useCallback(() => {
    void runDecision(() => props.onCancel({ artifact: props.artifact }));
  }, [props, runDecision]);

  const suggest = React.useCallback(() => {
    const trimmedNote = note.trim();
    if (!trimmedNote) {
      defer();
      return;
    }
    void runDecision(() => props.onSuggest({ artifact: props.artifact, note: trimmedNote }));
  }, [defer, note, props, runDecision]);

  const submitSelectedAction = React.useCallback(() => {
    if (selectedAction === "defer") {
      suggest();
      return;
    }
    execute();
  }, [execute, selectedAction, suggest]);

  const handleConfirmationKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (event.nativeEvent.isComposing || actionsDisabled) {
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
  }, [actionsDisabled, submitSelectedAction]);

  return (
    <section
      className="vk-plan"
      data-plan-artifact-pending={props.pending ? "true" : "false"}
      data-plan-artifact-expanded={expanded ? "true" : "false"}
    >
      <header className="vk-plan-head">
        <ClipboardList aria-hidden="true" />
        <strong>{props.artifact.name || t("planArtifact.titleFallback")}</strong>
        <div className="vk-actions">
          <IconButton
            aria-label={copied ? t("planArtifact.copied") : t("planArtifact.copy")}
            onClick={() => void handleCopy()}
            title={copied ? t("planArtifact.copied") : t("planArtifact.copy")}
            type="button"
          >
            {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </IconButton>
          <IconButton
            aria-expanded={expanded}
            aria-label={expanded ? t("planArtifact.collapse") : t("planArtifact.expand")}
            className="vk-plan-toggle"
            onClick={() => {
              setExpanded(!expanded);
            }}
            title={expanded ? t("planArtifact.collapse") : t("planArtifact.expand")}
            type="button"
          >
            <ChevronDown aria-hidden="true" />
          </IconButton>
        </div>
      </header>
      <div className="vk-plan-body">
        <ConversationMeasuredCollapse
          collapseId={`plan-artifact-body:${artifactKey}`}
          collapsedHeight={collapsedPreviewHeight}
          expanded={expanded}
        >
          <div className="vk-plan-doc">
            <MarkdownMessage content={props.artifact.content} eager />
            {!expanded ? (
              <Button
                className="vk-plan-reveal"
                onClick={() => {
                  setExpanded(true);
                }}
                size="sm"
                type="button"
                variant="outline"
              >
                {t("planArtifact.expand")}
              </Button>
            ) : null}
          </div>
        </ConversationMeasuredCollapse>
      </div>
      {props.pending ? (
        <div
          ref={confirmationRef}
          aria-activedescendant={`plan-artifact-action-${selectedAction}`}
          aria-label={t("planArtifact.confirmAria")}
          aria-busy={actionPending}
          className="vk-plan-confirm"
          onKeyDown={handleConfirmationKeyDown}
          tabIndex={0}
        >
          <textarea
            ref={noteInputRef}
            aria-label={t("planArtifact.noteAria")}
            className="vk-input vk-textarea vk-plan-note"
            data-density="compact"
            disabled={actionsDisabled}
            onChange={(event) => setNote(event.currentTarget.value)}
            onInput={(event) => resizePlanNoteInput(event.currentTarget)}
            placeholder={t("planArtifact.notePlaceholder")}
            rows={1}
            value={note}
          />
          <div className="vk-plan-choices">
            <Button
              id="plan-artifact-action-defer"
              aria-pressed={selectedAction === "defer"}
              className="vk-plan-choice"
              disabled={actionsDisabled}
              onClick={() => {
                setSelectedAction("defer");
                suggest();
              }}
              size="sm"
              type="button"
              variant="outline"
            >
              {noteHasContent ? <Send aria-hidden="true" /> : <X aria-hidden="true" />}
              <span>{noteHasContent ? t("planArtifact.suggest") : t("planArtifact.defer")}</span>
            </Button>
            <Button
              id="plan-artifact-action-execute"
              aria-pressed={selectedAction === "execute"}
              className="vk-plan-choice"
              disabled={actionsDisabled}
              onClick={() => {
                setSelectedAction("execute");
                execute();
              }}
              size="sm"
              type="button"
              variant="ember"
            >
              <PlayCircle aria-hidden="true" />
              <span>{noteHasContent ? t("planArtifact.executeWithNote") : t("planArtifact.execute")}</span>
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

// usePlanArtifactCollapsedPreviewHeight returns the stable preview height for collapsed Plan cards.
// usePlanArtifactCollapsedPreviewHeight 返回计划卡片收起态使用的稳定预览高度。
function usePlanArtifactCollapsedPreviewHeight(): number {
  // readPreviewHeight mirrors the previous CSS rule min(320px, 38vh).
  // readPreviewHeight 对齐原 CSS 规则 min(320px, 38vh)。
  const readPreviewHeight = React.useCallback(() => {
    if (typeof window === "undefined") {
      return 320;
    }
    return Math.max(160, Math.min(320, window.innerHeight * 0.38));
  }, []);
  const [height, setHeight] = React.useState(readPreviewHeight);
  React.useEffect(() => {
    const handleResize = () => setHeight(readPreviewHeight());
    handleResize();
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [readPreviewHeight]);
  return height;
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
