import React from "react";
import { RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ModelPickerControl, type ModelPickerChoice } from "@/components/composer/model-picker-control";
import { formatMessage, pickBilingualText, useI18n } from "@/i18n";
import { listKnowledgeLibraries } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, OutputStyleCatalogSummary, ProviderSummary } from "@/types";
import type { DedicatedDraftState } from "./dedicated-draft-store";

/** One selectable wizard answer.
 * 单个可选向导答案。 */
export interface DedicatedWizardOption<T> {
  value: T;
  label: string;
  hint?: string;
}

/** Renders mutually exclusive or multi-select answers as large pressable rows.
 * 将互斥或多选答案渲染为大号可按压行。 */
export function DedicatedWizardOptions<T>(props: {
  options: DedicatedWizardOption<T>[];
  isSelected: (value: T) => boolean;
  onSelect: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="dedicated-wizard-options">
      {props.options.map((option) => (
        <button
          aria-pressed={props.isSelected(option.value)}
          className="dedicated-wizard-option"
          disabled={props.disabled}
          key={String(option.value)}
          onClick={() => props.onSelect(option.value)}
          type="button"
        >
          <span>{option.label}</span>
          {option.hint ? <small>{option.hint}</small> : null}
        </button>
      ))}
    </div>
  );
}

/** Knowledge step: explicit libraries authorized for the project; none selected disables the knowledge base.
 * 知识库步骤：项目已授权的显式知识库；全部不选表示不使用知识库。 */
export function DedicatedKnowledgeStep(props: {
  projectId: string;
  libraryIds: string[];
  onChange: (libraryIds: string[]) => void;
}) {
  const { t } = useI18n();
  const [libraries, setLibraries] = React.useState<KnowledgeLibrarySummary[] | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const { onChange } = props;
  const selectedRef = React.useRef(props.libraryIds);
  selectedRef.current = props.libraryIds;
  React.useEffect(() => {
    let current = true;
    setLibraries(null);
    setError(null);
    listKnowledgeLibraries(props.projectId).then((next) => {
      if (!current) return;
      setLibraries(next);
      // Choices from another project never survive a project change.
      // 来自其他项目的选择不会在更换项目后保留。
      const available = new Set(next.map((library) => library.id));
      const kept = selectedRef.current.filter((id) => available.has(id));
      if (kept.length !== selectedRef.current.length) onChange(kept);
    }, (reason: unknown) => {
      if (current) setError(String(reason));
    });
    return () => { current = false; };
  }, [onChange, props.projectId]);
  if (error) return <p className="dedicated-wizard-error" role="alert">{error}</p>;
  if (libraries === null) return <p className="dedicated-wizard-hint" role="status">{t("composer.dedicated.knowledgeLoading")}</p>;
  const options: DedicatedWizardOption<string | null>[] = [
    { value: null, label: t("composer.dedicated.knowledgeNone"), hint: t("composer.dedicated.knowledgeNoneHint") },
    ...libraries.map((library) => ({ value: library.id, label: library.name, hint: library.description || undefined })),
  ];
  return (
    <DedicatedWizardOptions
      isSelected={(value) => value === null ? props.libraryIds.length === 0 : props.libraryIds.includes(value)}
      onSelect={(value) => {
        if (value === null) props.onChange([]);
        else if (props.libraryIds.includes(value)) props.onChange(props.libraryIds.filter((id) => id !== value));
        else props.onChange([...props.libraryIds, value]);
      }}
      options={options}
    />
  );
}

/** Output-style step: follow the global selection or pick one catalog entry.
 * 输出风格步骤：跟随全局选择或选择一个目录项。 */
export function DedicatedOutputStyleStep(props: {
  catalog: OutputStyleCatalogSummary;
  styleId: string | null;
  onChange: (styleId: string | null) => void;
}) {
  const { locale, t } = useI18n();
  const globalEntry = props.catalog.entries.find((entry) => entry.id === props.catalog.selectedId);
  const globalLabel = globalEntry ? pickBilingualText(locale, globalEntry.labelZh, globalEntry.labelEn) : props.catalog.selectedId;
  const options: DedicatedWizardOption<string | null>[] = [
    {
      value: null,
      label: t("composer.outputStyle.inheritGlobal"),
      hint: formatMessage(t("composer.outputStyle.useSystemSetting"), { label: globalLabel }),
    },
    ...props.catalog.entries.map((entry) => ({
      value: entry.id,
      label: pickBilingualText(locale, entry.labelZh, entry.labelEn),
      hint: formatMessage(t("composer.outputStyle.promptCharacters"), { count: entry.promptCharCount.toLocaleString(locale) }),
    })),
  ];
  return <DedicatedWizardOptions isSelected={(value) => value === props.styleId} onSelect={props.onChange} options={options} />;
}

/** Requirement step: describe the job and choose the model that drafts the prompt.
 * 需求步骤：描述职责并选择起草提示词的模型。 */
export function DedicatedRequirementStep(props: {
  state: DedicatedDraftState;
  providers: ProviderSummary[];
  busy: boolean;
  onRequirementChange: (requirement: string) => void;
  onSelectModel: (choice: ModelPickerChoice) => void;
}) {
  const { t } = useI18n();
  return (
    <>
      <Textarea
        aria-label={t("composer.dedicated.requirement")}
        density="large"
        disabled={props.busy}
        onChange={(event) => props.onRequirementChange(event.target.value)}
        placeholder={t("composer.dedicated.requirementPlaceholder")}
        value={props.state.requirement}
      />
      <div className="dedicated-wizard-actions">
        <span className="dedicated-wizard-hint">{t("composer.dedicated.generatorModel")}</span>
        <ModelPickerControl
          ariaLabel={t("composer.aria.modelReasoning")}
          disabled={props.busy}
          fallbackLabel={t("composer.modelPicker.placeholder")}
          filterPlaceholder={t("composer.modelPicker.filter")}
          noMatchesLabel={t("composer.modelPicker.noMatches")}
          onValueChange={props.onSelectModel}
          providers={props.providers}
          value=""
        />
      </div>
    </>
  );
}

/** Prompt step: edit the drafted prompt directly or revise it with supplementary instructions.
 * 提示词步骤：直接编辑起草的提示词，或以补充说明进行修订。 */
export function DedicatedPromptStep(props: {
  state: DedicatedDraftState;
  busy: boolean;
  canGenerate: boolean;
  onPromptChange: (prompt: string) => void;
  onRevisionChange: (revisionRequest: string) => void;
  onRevise: () => void;
}) {
  const { t } = useI18n();
  return (
    <>
      <Textarea
        aria-label={t("composer.dedicated.prompt")}
        density="large"
        disabled={props.busy}
        onChange={(event) => props.onPromptChange(event.target.value)}
        placeholder={t("composer.dedicated.promptPlaceholder")}
        value={props.state.prompt}
      />
      <p className="dedicated-wizard-hint">{t("composer.dedicated.promptImmutableHint")}</p>
      <Textarea
        aria-label={t("composer.dedicated.revision")}
        density="compact"
        disabled={props.busy}
        onChange={(event) => props.onRevisionChange(event.target.value)}
        placeholder={t("composer.dedicated.revisionPlaceholder")}
        value={props.state.revisionRequest}
      />
      <div className="dedicated-wizard-actions">
        <Button
          disabled={props.busy || !props.canGenerate || props.state.prompt.trim().length === 0 || props.state.revisionRequest.trim().length === 0}
          onClick={props.onRevise}
          size="sm"
          type="button"
          variant="outline"
        >
          {props.busy ? <RefreshCw className="animate-spin" size={14} /> : <Sparkles size={14} />}
          {t("composer.dedicated.revise")}
        </Button>
      </div>
    </>
  );
}
