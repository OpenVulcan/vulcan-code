import React from "react";
import { ArrowLeft, ArrowRight, PenLine, RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import { composerSessionDraftKey } from "@/components/composer/composer-panel/helpers";
import { modelPickerChoicesFromProviders, type ModelPickerChoice } from "@/components/composer/model-picker-control";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { AnswerMode } from "@/service/answer-mode";
import { draftDedicatedPrompt } from "@/service/dedicated-session-service";
import type { OutputStyleCatalogSummary, ProviderSummary } from "@/types";
import { dedicatedErrorMessage } from "./dedicated-errors";
import {
  DEDICATED_WIZARD_STEPS,
  writeDedicatedDraft,
  type DedicatedDraftState,
  type DedicatedWizardStep,
} from "./dedicated-draft-store";
import type { DraftComposerContextValue } from "./draft-composer-context";
import {
  DedicatedKnowledgeStep,
  DedicatedOutputStyleStep,
  DedicatedPromptStep,
  DedicatedRequirementStep,
  DedicatedWizardOptions,
} from "./dedicated-wizard-steps";

// STEP_TITLE_KEYS names each wizard step.
// STEP_TITLE_KEYS 为每个向导步骤命名。
const STEP_TITLE_KEYS: Record<DedicatedWizardStep, TranslationKey> = {
  knowledge: "composer.dedicated.step.knowledge",
  answerMode: "composer.dedicated.step.answerMode",
  outputStyle: "composer.dedicated.step.outputStyle",
  requirement: "composer.dedicated.step.requirement",
  prompt: "composer.dedicated.step.prompt",
  memory: "composer.dedicated.step.memory",
};

/** Sequential creation wizard shown instead of the text composer when a draft becomes a dedicated assistant.
 * 草稿切换为专职助手时取代文本输入区显示的顺序创建向导。
 *
 * @param props.draft workbench draft facts; props.state and props.onChange own the local choices.
 *                    工作台草稿事实；props.state 与 props.onChange 持有本地选择。
 * @param props.onSubmitPrompt creates the session; the base prompt becomes the first, labelled user message. */
export function DedicatedSessionWizard(props: {
  draft: DraftComposerContextValue;
  state: DedicatedDraftState;
  onChange: (update: (current: DedicatedDraftState) => DedicatedDraftState) => void;
  providers: ProviderSummary[];
  outputStyleCatalog: OutputStyleCatalogSummary;
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  onBusyChange: (busy: boolean) => void;
}) {
  const { t } = useI18n();
  const { state, onChange, onBusyChange } = props;
  const [busy, setBusy] = React.useState<"draft" | "submit" | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => onBusyChange(busy === "submit"), [busy, onBusyChange]);
  // activeChoice is the exact route the composer would use; drafting and creation never substitute it.
  // activeChoice 是输入区将使用的精确路由；起草与创建都不会替换它。
  const activeChoice = React.useMemo(
    () => modelPickerChoicesFromProviders(props.providers).find((choice) => choice.active) ?? null,
    [props.providers],
  );
  const stepIndex = DEDICATED_WIZARD_STEPS.indexOf(state.step);
  const goTo = (step: DedicatedWizardStep) => {
    setError(null);
    onChange((current) => ({ ...current, step }));
  };
  const setKnowledge = React.useCallback(
    (knowledgeLibraryIds: string[]) => onChange((current) => ({ ...current, knowledgeLibraryIds })),
    [onChange],
  );

  const generate = async (revise: boolean) => {
    if (!activeChoice || busy) return;
    setBusy("draft");
    setError(null);
    try {
      const response = await draftDedicatedPrompt({
        providerId: activeChoice.providerId,
        connectionId: activeChoice.connectionId,
        modelId: activeChoice.modelId,
        requirement: state.requirement,
        currentPrompt: revise ? state.prompt : undefined,
        revisionRequest: revise ? state.revisionRequest : undefined,
      });
      onChange((current) => ({ ...current, prompt: response.prompt, revisionRequest: "", step: "prompt" }));
    } catch (reason) {
      setError(dedicatedErrorMessage(reason, t));
    } finally {
      setBusy(null);
    }
  };

  const create = async (lockImmediately: boolean) => {
    const basePrompt = state.prompt.trim();
    if (!basePrompt || busy) return;
    setBusy("submit");
    setError(null);
    try {
      const submitted = await props.onSubmitPrompt({
        draftSlotKey: composerSessionDraftKey(props.draft.projectId, null),
        prompt: basePrompt,
        attachments: [],
        executionMode: "build",
        reasoningEffort: null,
        modelSelection: activeChoice
          ? { providerId: activeChoice.providerId, modelId: activeChoice.modelId, connectionId: activeChoice.connectionId }
          : null,
        sessionSettings: {
          ...(state.answerMode ? { answerMode: state.answerMode } : {}),
          ...(state.outputStyleId ? { outputStyleId: state.outputStyleId } : {}),
          knowledgeSelection: { mode: "explicit", libraryIds: state.knowledgeLibraryIds },
          dedicated: { toolAccess: state.toolAccess, basePrompt, lockImmediately },
        },
      });
      if (submitted) writeDedicatedDraft(props.draft.draftId, null);
    } finally {
      setBusy(null);
    }
  };

  const answerModeOptions: { value: AnswerMode | null; label: string; hint: string }[] = [
    { value: null, label: t("composer.dedicated.followDefault"), hint: t("composer.dedicated.followDefaultHint") },
    { value: "standard", label: t("answerMode.standard"), hint: t("answerMode.standardHint") },
    { value: "interactive", label: t("answerMode.interactive"), hint: t("answerMode.interactiveHint") },
  ];
  const canAdvance = state.step !== "prompt" || state.prompt.trim().length > 0;
  const next = DEDICATED_WIZARD_STEPS[stepIndex + 1];
  const previous = DEDICATED_WIZARD_STEPS[stepIndex - 1];

  return (
    <section aria-busy={busy !== null} aria-label={t("composer.draft.kindDedicated")} className="dedicated-wizard">
      <header className="dedicated-wizard-header">
        <strong>{t(STEP_TITLE_KEYS[state.step])}</strong>
        <small>{formatMessage(t("composer.dedicated.stepCount"), { current: stepIndex + 1, total: DEDICATED_WIZARD_STEPS.length })}</small>
      </header>
      {state.step === "knowledge" ? (
        <DedicatedKnowledgeStep libraryIds={state.knowledgeLibraryIds} onChange={setKnowledge} projectId={props.draft.projectId} />
      ) : null}
      {state.step === "answerMode" ? (
        <DedicatedWizardOptions
          isSelected={(value) => value === state.answerMode}
          onSelect={(answerMode) => onChange((current) => ({ ...current, answerMode }))}
          options={answerModeOptions}
        />
      ) : null}
      {state.step === "outputStyle" ? (
        <DedicatedOutputStyleStep
          catalog={props.outputStyleCatalog}
          onChange={(outputStyleId) => onChange((current) => ({ ...current, outputStyleId }))}
          styleId={state.outputStyleId}
        />
      ) : null}
      {state.step === "requirement" ? (
        <DedicatedRequirementStep
          busy={busy !== null}
          onRequirementChange={(requirement) => onChange((current) => ({ ...current, requirement }))}
          onSelectModel={(choice) => void props.onSelectWorkbenchModel(choice)}
          providers={props.providers}
          state={state}
        />
      ) : null}
      {state.step === "prompt" ? (
        <DedicatedPromptStep
          busy={busy !== null}
          canGenerate={activeChoice !== null}
          onPromptChange={(prompt) => onChange((current) => ({ ...current, prompt }))}
          onRevise={() => void generate(true)}
          onRevisionChange={(revisionRequest) => onChange((current) => ({ ...current, revisionRequest }))}
          state={state}
        />
      ) : null}
      {state.step === "memory" ? (
        <DedicatedWizardOptions
          disabled={busy !== null}
          isSelected={() => false}
          onSelect={(lockImmediately) => void create(lockImmediately)}
          options={[
            { value: true, label: t("composer.dedicated.lockNow"), hint: t("composer.dedicated.lockNowHint") },
            { value: false, label: t("composer.dedicated.lockLater"), hint: t("composer.dedicated.lockLaterHint") },
          ]}
        />
      ) : null}
      {error ? <p className="dedicated-wizard-error" role="alert">{error}</p> : null}
      {busy === "submit" ? <p className="dedicated-wizard-hint" role="status">{t("composer.dedicated.creating")}</p> : null}
      <div className="dedicated-wizard-actions">
        {previous ? (
          <Button disabled={busy !== null} onClick={() => goTo(previous)} size="sm" type="button" variant="ghost">
            <ArrowLeft size={14} />
            {t("composer.dedicated.back")}
          </Button>
        ) : null}
        <span className="dedicated-wizard-actions-end">
          {state.step === "requirement" ? (
            <>
              <Button disabled={busy !== null} onClick={() => goTo("prompt")} size="sm" type="button" variant="outline">
                <PenLine size={14} />
                {t("composer.dedicated.writeOwn")}
              </Button>
              <Button
                disabled={busy !== null || !activeChoice || state.requirement.trim().length === 0}
                onClick={() => void generate(false)}
                size="sm"
                type="button"
              >
                {busy === "draft" ? <RefreshCw className="animate-spin" size={14} /> : <Sparkles size={14} />}
                {t("composer.dedicated.generate")}
              </Button>
            </>
          ) : next ? (
            <Button disabled={busy !== null || !canAdvance} onClick={() => goTo(next)} size="sm" type="button">
              {t("composer.dedicated.next")}
              <ArrowRight size={14} />
            </Button>
          ) : null}
        </span>
      </div>
    </section>
  );
}

