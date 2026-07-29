import React from "react";
import { toast } from "sonner";
import type {
  ComposerModelSelectionSnapshot,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import type { ComposerTraceCallback } from "@/components/composer/composer-control-types";
import type { ComposerDraftSubmissionSnapshot } from "@/components/composer/composer-panel/use-composer-session-draft";
import { useI18n } from "@/i18n";
import type { ComposerAttachmentSummary } from "@/types";

// ComposerSubmissionSettings contains runtime controls shared by ordinary and goal-created submissions.
// ComposerSubmissionSettings 包含普通提交与目标创建提交共享的运行时控件。
type ComposerSubmissionSettings = Pick<
  ComposerSubmission,
  "executionMode" | "reasoningEffort" | "modelSelection"
>;

// UseComposerSendControllerOptions defines draft state, runtime controls, and backend actions for sending.
// UseComposerSendControllerOptions 定义发送所需的草稿状态、运行时控件与后端操作。
export interface UseComposerSendControllerOptions {
  // Stable draft slot that owns generated-submission failure recovery.
  // 持有生成式提交失败恢复的稳定草稿槽。
  draftSlotKey: string;
  // Current unsent composer text.
  // 当前尚未发送的输入区文本。
  draftInput: string;
  // Current staged composer attachments.
  // 当前已暂存的输入区附件。
  attachments: ComposerAttachmentSummary[];
  // Current execution mode included in submissions.
  // 提交中包含的当前执行模式。
  executionMode: string;
  // Backend-confirmed reasoning effort included in submissions.
  // 提交中包含的后端确认推理档位。
  reasoningEffort: string | null;
  // Exact model and account currently rendered by the composer.
  // 输入区当前渲染的精确模型与账号。
  modelSelection: ComposerModelSelectionSnapshot | null;
  // Captures the latest draft-only session settings at the exact submission boundary.
  // 在精确提交边界捕获最新草稿专属会话设置。
  captureSessionSettings: () => ComposerSubmission["sessionSettings"];
  // Consumes the captured draft settings after the backend accepts the submission.
  // 在后端接受提交后消费已捕获的草稿设置。
  clearSessionSettings: () => void;
  // Flushes pending provider/model configuration writes before submission.
  // 在提交前刷新仍在进行的供应商/模型配置写入。
  beforeSubmit?: () => Promise<boolean>;
  // Whether the selected backend conversation currently has an active run.
  // 当前选中的后端会话是否存在活动运行。
  isConversationRunning: boolean;
  // Captures and clears the active draft before an ordinary send.
  // 在普通发送前捕获并清理当前草稿。
  clearForSubmission: () => ComposerDraftSubmissionSnapshot;
  // Restores a failed ordinary send to its owning draft slot.
  // 将失败的普通发送恢复到所属草稿槽。
  restoreFailedSubmission: (snapshot: ComposerDraftSubmissionSnapshot) => void;
  // Restores failed generated text to its owning draft slot.
  // 将失败的生成文本恢复到所属草稿槽。
  restoreInputToSlot: (draftSlotKey: string, input: string) => void;
  // Sends one complete composer submission through the workbench runtime.
  // 通过工作台运行时发送一条完整输入区提交。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Stops the selected backend conversation run.
  // 停止当前选中的后端会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Optional local UI trace callback.
  // 可选的本地界面追踪回调。
  onTrace?: ComposerTraceCallback;
}

// ComposerSendController exposes the single send/stop transaction boundary consumed by ComposerPanel.
// ComposerSendController 暴露 ComposerPanel 使用的唯一发送/停止事务边界。
export interface ComposerSendController {
  // Whether a new goal may automatically submit its first backend turn.
  // 新目标是否可以自动提交第一个后端轮次。
  canAutoSubmitCreatedGoal: boolean;
  // Validates, consumes, and sends the current draft or stops an empty running conversation.
  // 校验、消费并发送当前草稿，或停止空草稿的运行中会话。
  sendDraft: () => Promise<void>;
  // Stops the current backend run without consuming draft content.
  // 在不消费草稿内容的情况下停止当前后端运行。
  stopConversation: () => Promise<void>;
  // Sends the first turn for a newly persisted goal when no competing run has started.
  // 在没有竞争运行启动时发送新持久化目标的首轮。
  submitCreatedGoal: (objective: string) => Promise<void>;
}

// useComposerSendController owns submission gating, draft transactions, stop routing, and failure recovery.
// useComposerSendController 管理提交门控、草稿事务、停止路由与失败恢复。
// Parameters: options provides current draft/runtime state and confirmed backend action callbacks.
// 参数：options 提供当前草稿/运行时状态与已确认的后端操作回调。
// Returns: stable ordinary-send, stop, generated-goal send, and auto-submit eligibility actions.
// 返回值：稳定的普通发送、停止、目标生成发送与自动提交资格操作。
export function useComposerSendController(options: UseComposerSendControllerOptions): ComposerSendController {
  // t resolves stable composer error messages for the current locale.
  // t 为当前语言解析稳定的输入区错误消息。
  const { t } = useI18n();
  // isSubmitting drives React-visible eligibility updates for goal auto submission.
  // isSubmitting 驱动目标自动提交资格的 React 可见更新。
  const [isSubmitting, setIsSubmitting] = React.useState(false);
  // submittingRef synchronously blocks duplicate sends before React state commits.
  // submittingRef 在 React 状态提交前同步拦截重复发送。
  const submittingRef = React.useRef(false);
  // submissionVersionRef advances whenever any local submission acquires the gate.
  // submissionVersionRef 在任一本地提交获取门控时递增。
  const submissionVersionRef = React.useRef(0);
  // submissionVersionAtRender lets retained goal-save callbacks detect a later competing submission.
  // submissionVersionAtRender 让保留的目标保存回调检测后续竞争提交。
  const submissionVersionAtRender = submissionVersionRef.current;
  // conversationRunningRef exposes the latest backend run state to callbacks retained across goal saves.
  // conversationRunningRef 向跨目标保存保留的回调暴露最新后端运行状态。
  const conversationRunningRef = React.useRef(options.isConversationRunning);
  conversationRunningRef.current = options.isConversationRunning;
  // submissionSettings combines runtime controls shared by both submission paths.
  // submissionSettings 合并两条提交路径共享的运行时控件。
  const submissionSettings = React.useMemo<ComposerSubmissionSettings>(() => ({
    executionMode: options.executionMode,
    reasoningEffort: options.reasoningEffort,
    modelSelection: options.modelSelection,
  }), [
    options.executionMode,
    options.modelSelection,
    options.reasoningEffort,
  ]);
  // canAutoSubmitCreatedGoal reflects current render state while submitCreatedGoal also performs a synchronous guard.
  // canAutoSubmitCreatedGoal 反映当前渲染状态，submitCreatedGoal 同时执行同步门控。
  const canAutoSubmitCreatedGoal = !options.isConversationRunning && !isSubmitting;

  // beginSubmission acquires the component-wide submission gate synchronously.
  // beginSubmission 同步获取组件范围内的提交门控。
  const beginSubmission = React.useCallback(() => {
    if (submittingRef.current) {
      return false;
    }
    submittingRef.current = true;
    submissionVersionRef.current += 1;
    setIsSubmitting(true);
    return true;
  }, []);
  // finishSubmission releases both synchronous and React-visible submission gates.
  // finishSubmission 释放同步门控与 React 可见提交门控。
  const finishSubmission = React.useCallback(() => {
    submittingRef.current = false;
    setIsSubmitting(false);
  }, []);
  // requestStop routes one stop request and optionally records an explicit stop-button trace.
  // requestStop 路由一次停止请求，并可选记录显式停止按钮追踪。
  const requestStop = React.useCallback(async (traceExplicitStop: boolean) => {
    if (traceExplicitStop) {
      options.onTrace?.("composer.stop", {
        isConversationRunning: options.isConversationRunning,
        draftChars: options.draftInput.trim().length,
        attachmentCount: options.attachments.length,
      });
    }
    // stopped records whether the backend completed or confirmed the requested stop flow.
    // stopped 记录后端是否完成或确认请求的停止流程。
    const stopped = await options.onStopConversationRun();
    if (!stopped) {
      toast.error(t("composer.error.stopCurrentFailed"));
    }
  }, [
    options.attachments.length,
    options.draftInput,
    options.isConversationRunning,
    options.onStopConversationRun,
    options.onTrace,
    t,
  ]);

  // sendDraft validates and consumes the active draft before forwarding it to the backend runtime.
  // sendDraft 校验并消费当前草稿，再将其转发到后端运行时。
  const sendDraft = React.useCallback(async () => {
    // prompt is the trimmed model-facing text while the transaction snapshot preserves exact input.
    // prompt 是去除首尾空白的模型文本，事务快照仍保留精确输入。
    const prompt = options.draftInput.trim();
    // hasSubmittableContent includes either non-empty text or at least one staged attachment.
    // hasSubmittableContent 包含非空文本或至少一个已暂存附件。
    const hasSubmittableContent = Boolean(prompt || options.attachments.length > 0);
    options.onTrace?.("composer.send_or_stop", {
      isConversationRunning: options.isConversationRunning,
      hasSubmittableContent,
      promptChars: prompt.length,
      attachmentCount: options.attachments.length,
    });
    if (options.isConversationRunning && !hasSubmittableContent) {
      await requestStop(false);
      return;
    }
    if (submittingRef.current) {
      return;
    }
    if (!hasSubmittableContent) {
      toast.info(t("composer.error.promptEmpty"));
      return;
    }
    if (!beginSubmission()) {
      return;
    }
    try {
      if (options.beforeSubmit && !(await options.beforeSubmit())) {
        return;
      }
      // submittedDraft binds failure recovery to the exact slot consumed by this send.
      // submittedDraft 将失败恢复绑定到本次发送消费的精确草稿槽。
      const submittedDraft = options.clearForSubmission();
      // sent records whether the workbench runtime accepted this complete submission.
      // sent 记录工作台运行时是否接受本次完整提交。
      const sent = await options.onSubmitPrompt({
        prompt,
        attachments: submittedDraft.attachments,
        ...submissionSettings,
        sessionSettings: options.captureSessionSettings(),
      });
        if (!sent) {
          options.restoreFailedSubmission(submittedDraft);
        } else {
          options.clearSessionSettings();
        }
    } finally {
      finishSubmission();
    }
  }, [
    beginSubmission,
    finishSubmission,
    options.attachments.length,
    options.beforeSubmit,
    options.captureSessionSettings,
    options.clearSessionSettings,
    options.clearForSubmission,
    options.draftInput,
    options.isConversationRunning,
    options.onSubmitPrompt,
    options.onTrace,
    options.restoreFailedSubmission,
    requestStop,
    submissionSettings,
    t,
  ]);
  // stopConversation stops the active backend run while retaining every draft field.
  // stopConversation 停止当前后端运行，同时保留全部草稿字段。
  const stopConversation = React.useCallback(async () => {
    await requestStop(true);
  }, [requestStop]);
  // submitCreatedGoal sends a persisted goal objective unless another submission or backend run has started.
  // submitCreatedGoal 在没有其他提交或后端运行启动时发送已持久化目标文本。
  const submitCreatedGoal = React.useCallback(async (objective: string) => {
    if (
      submissionVersionRef.current !== submissionVersionAtRender
      || submittingRef.current
      || conversationRunningRef.current
      || !beginSubmission()
    ) {
      return;
    }
    // submissionDraftSlotKey binds failure recovery to the goal owner's original draft slot.
    // submissionDraftSlotKey 将失败恢复绑定到目标所属的原始草稿槽。
    const submissionDraftSlotKey = options.draftSlotKey;
    try {
      if (options.beforeSubmit && !(await options.beforeSubmit())) {
        return;
      }
      // sent records whether the workbench runtime accepted the generated first turn.
      // sent 记录工作台运行时是否接受生成的首轮。
      const sent = await options.onSubmitPrompt({
        prompt: objective,
        attachments: [],
        ...submissionSettings,
        sessionSettings: options.captureSessionSettings(),
      });
      if (!sent) {
        options.restoreInputToSlot(submissionDraftSlotKey, objective);
      } else {
        options.clearSessionSettings();
      }
    } finally {
      finishSubmission();
    }
  }, [
    beginSubmission,
    finishSubmission,
    options.draftSlotKey,
    options.beforeSubmit,
    options.captureSessionSettings,
    options.clearSessionSettings,
    options.onSubmitPrompt,
    options.restoreInputToSlot,
    submissionSettings,
    submissionVersionAtRender,
  ]);

  return {
    canAutoSubmitCreatedGoal,
    sendDraft,
    stopConversation,
    submitCreatedGoal,
  };
}
