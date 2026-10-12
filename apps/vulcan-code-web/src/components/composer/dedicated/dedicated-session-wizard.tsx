import React from "react";
import { ArrowLeft, ArrowRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import { composerSessionDraftKey } from "@/components/composer/composer-panel/helpers";
import { useComposerModelSelection } from "@/components/composer/composer-panel/use-composer-model-selection";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import { ModelReasoningPickerControl } from "@/components/composer/model-reasoning-picker-control";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { OutputStyleCatalogSummary, ProviderSummary } from "@/types";
import {
  DEDICATED_RECOMMENDED_ANSWER_MODE,
  DEDICATED_RECOMMENDED_OUTPUT_STYLE_ID,
  DEDICATED_WIZARD_STEPS,
  writeDedicatedDraft,
  type DedicatedDraftState,
  type DedicatedWizardStep,
} from "./dedicated-draft-store";
import type { DraftComposerContextValue } from "./draft-composer-context";
import { DedicatedPromptGeneratorDialog } from "./dedicated-prompt-generator-dialog";
import {
  DedicatedPromptStep,
  DedicatedSettingsStep,
  DedicatedWizardOptions,
  useProjectKnowledgeLibraries,
} from "./dedicated-wizard-steps";
import "@/components/composer/dedicated/dedicated-session-wizard.css";

// STEP_TITLE_KEYS names each wizard step.
// STEP_TITLE_KEYS 为每个向导步骤命名。
const STEP_TITLE_KEYS: Record<DedicatedWizardStep, TranslationKey> = {
  settings: "composer.dedicated.step.settings",
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
  const [submitting, setSubmitting] = React.useState(false);
  const [generatorOpen, setGeneratorOpen] = React.useState(false);
  React.useEffect(() => onBusyChange(submitting), [submitting, onBusyChange]);
  const draftSlotKey = composerSessionDraftKey(props.draft.projectId, null);
  // The session model and thinking level follow the composer's draft rules: the model choice is the workbench
  // route, the level stays local and defaults to the model's declared level until the session is created.
  // 会话模型与思考档位遵循输入区草稿规则：模型选择即工作台路由，档位保持本地并默认取模型声明的档位，直到会话创建。
  const model = useComposerModelSelection({
    conversationScopeKey: draftSlotKey,
    persistReasoningSelection: false,
    providers: props.providers,
    reasoningSelection: null,
    onSelectModel: props.onSelectWorkbenchModel,
    onSelectReasoningEffort: () => Promise.resolve(true),
  });
  const activeChoice = model.activeModelChoice;
  const setKnowledge = React.useCallback(
    (knowledgeLibraryIds: string[]) => onChange((current) => ({ ...current, knowledgeLibraryIds })),
    [onChange],
  );
  const knowledge = useProjectKnowledgeLibraries(props.draft.projectId, state.knowledgeLibraryIds, setKnowledge);
  // Unpicked settings show and submit the recommended values, never the global ones, so creation copies what is shown.
  // 未选择的设置显示并提交推荐值而非全局值，使创建结果与所见一致。
  const answerMode = state.answerMode ?? DEDICATED_RECOMMENDED_ANSWER_MODE;
  const outputStyleId = state.outputStyleId ?? DEDICATED_RECOMMENDED_OUTPUT_STYLE_ID;
  const step = state.step;
  const stepIndex = DEDICATED_WIZARD_STEPS.indexOf(step);
  const previous = DEDICATED_WIZARD_STEPS[stepIndex - 1];
  const next = DEDICATED_WIZARD_STEPS[stepIndex + 1];
  const goTo = (target: DedicatedWizardStep) => onChange((current) => ({ ...current, step: target }));
  const canAdvance = step !== "prompt" || state.prompt.trim().length > 0;

  const create = async (lockImmediately: boolean) => {
    const basePrompt = state.prompt.trim();
    if (!basePrompt || submitting) return;
    setSubmitting(true);
    try {
      const submitted = await props.onSubmitPrompt({
        draftSlotKey,
        prompt: basePrompt,
        attachments: [],
        executionMode: "build",
        reasoningEffort: model.submittedReasoningEffort,
        modelSelection: activeChoice
          ? { providerId: activeChoice.providerId, modelId: activeChoice.modelId, connectionId: activeChoice.connectionId }
          : null,
        sessionSettings: {
          answerMode,
          outputStyleId,
          knowledgeSelection: { mode: "explicit", libraryIds: state.knowledgeLibraryIds },
          dedicated: { toolAccess: state.toolAccess, basePrompt, lockImmediately },
        },
      });
      if (submitted) writeDedicatedDraft(props.draft.draftId, null);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <section aria-busy={submitting} aria-label={t("composer.draft.kindDedicated")} className="dedicated-wizard">
      <header className="dedicated-wizard-header">
        <strong>{t(STEP_TITLE_KEYS[step])}</strong>
        <small>{formatMessage(t("composer.dedicated.stepCount"), { current: stepIndex + 1, total: DEDICATED_WIZARD_STEPS.length })}</small>
      </header>
      <div className="dedicated-wizard-body">
        {step === "settings" ? (
          <DedicatedSettingsStep
            answerMode={answerMode}
            catalog={props.outputStyleCatalog}
            knowledge={knowledge}
            libraryIds={state.knowledgeLibraryIds}
            onAnswerModeChange={(value) => onChange((current) => ({ ...current, answerMode: value }))}
            onKnowledgeChange={setKnowledge}
            onStyleChange={(value) => onChange((current) => ({ ...current, outputStyleId: value }))}
            styleId={outputStyleId}
          />
        ) : null}
        {step === "prompt" ? (
          <DedicatedPromptStep
            canGenerate={activeChoice !== null}
            disabled={submitting}
            onOpenGenerator={() => setGeneratorOpen(true)}
            modelControl={(
              <ModelReasoningPickerControl
                accountAriaLabel={t("composer.aria.account")}
                accounts={model.activeModelAccounts}
                accountValue={activeChoice?.connectionId ?? ""}
                ariaLabel={t("composer.aria.modelReasoning")}
                defaultReasoningEffort={model.defaultReasoningEffort}
                disabled={submitting}
                fallbackLabel={t("composer.modelPicker.placeholder")}
                filterPlaceholder={t("composer.modelPicker.filter")}
                noMatchesLabel={t("composer.modelPicker.noMatches")}
                onAccountChange={(choice) => void model.selectModel(choice)}
                onModelChange={(choice) => void model.selectModel(choice)}
                onReasoningEffortChange={(value) => void model.selectReasoningEffort(value)}
                providers={props.providers}
                reasoningEffort={model.reasoningEffort}
                reasoningOptions={model.reasoningEffortOptions}
                value={model.modelChoice}
              />
            )}
            onPromptChange={(prompt) => onChange((current) => ({ ...current, prompt }))}
            prompt={state.prompt}
          />
        ) : null}
        {step === "memory" ? (
          <DedicatedWizardOptions
            disabled={submitting}
            isSelected={() => false}
            onSelect={(lockImmediately) => void create(lockImmediately)}
            options={[
              { value: true, label: t("composer.dedicated.lockNow"), hint: t("composer.dedicated.lockNowHint") },
              { value: false, label: t("composer.dedicated.lockLater"), hint: t("composer.dedicated.lockLaterHint") },
            ]}
          />
        ) : null}
      </div>
      {submitting ? <p className="dedicated-wizard-hint" role="status">{t("composer.dedicated.creating")}</p> : null}
      <div className="dedicated-wizard-actions">
        {previous ? (
          <Button disabled={submitting} onClick={() => goTo(previous)} size="sm" type="button" variant="ghost">
            <ArrowLeft size={14} />
            {t("composer.dedicated.back")}
          </Button>
        ) : null}
        {next ? (
          <span className="dedicated-wizard-actions-end">
            <Button disabled={submitting || !canAdvance} onClick={() => goTo(next)} size="sm" type="button">
              {t("composer.dedicated.next")}
              <ArrowRight size={14} />
            </Button>
          </span>
        ) : null}
      </div>
      <DedicatedPromptGeneratorDialog
        currentPrompt={state.prompt}
        onApply={(prompt) => {
          onChange((current) => ({ ...current, prompt }));
          setGeneratorOpen(false);
        }}
        onOpenChange={setGeneratorOpen}
        onRequirementChange={(requirement) => onChange((current) => ({ ...current, requirement }))}
        open={generatorOpen}
        providers={props.providers}
        requirement={state.requirement}
      />
    </section>
  );
}
