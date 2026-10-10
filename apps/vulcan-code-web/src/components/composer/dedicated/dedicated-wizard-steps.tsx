import React from "react";
import { Brain, RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { ModelPickerControl, type ModelPickerChoice } from "@/components/composer/model-picker-control";
import { reasoningLevelLabel } from "@/components/composer/model-reasoning-picker-control";
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

/** Renders mutually exclusive or multi-select answers as a grid of pressable cards.
 * 将互斥或多选答案渲染为可按压卡片矩阵。 */
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

/** Loaded knowledge libraries of the draft's project; null while loading.
 * 草稿所属项目已加载的知识库；加载中为 null。 */
export interface ProjectKnowledgeLibraries {
  libraries: KnowledgeLibrarySummary[] | null;
  error: string | null;
}

/** Loads the project's explicit knowledge libraries and drops selections from another project.
 * 加载项目的显式知识库，并丢弃来自其他项目的选择。
 *
 * @param projectId draft project; reloading starts whenever it changes.
 *                  草稿所属项目；变化时重新加载。
 * @param libraryIds current selection; onChange receives the pruned selection. */
export function useProjectKnowledgeLibraries(
  projectId: string,
  libraryIds: string[],
  onChange: (libraryIds: string[]) => void,
): ProjectKnowledgeLibraries {
  const [state, setState] = React.useState<ProjectKnowledgeLibraries>({ libraries: null, error: null });
  const selectedRef = React.useRef(libraryIds);
  selectedRef.current = libraryIds;
  React.useEffect(() => {
    let current = true;
    setState({ libraries: null, error: null });
    listKnowledgeLibraries(projectId).then((next) => {
      if (!current) return;
      setState({ libraries: next, error: null });
      // Choices from another project never survive a project change.
      // 来自其他项目的选择不会在更换项目后保留。
      const available = new Set(next.map((library) => library.id));
      const kept = selectedRef.current.filter((id) => available.has(id));
      if (kept.length !== selectedRef.current.length) onChange(kept);
    }, (reason: unknown) => {
      if (current) setState({ libraries: null, error: String(reason) });
    });
    return () => { current = false; };
  }, [onChange, projectId]);
  return state;
}

/** Knowledge step: explicit libraries authorized for the project; none selected disables the knowledge base.
 * 知识库步骤：项目已授权的显式知识库；全部不选表示不使用知识库。
 * The wizard omits this step when the project has no library. */
export function DedicatedKnowledgeStep(props: {
  knowledge: ProjectKnowledgeLibraries;
  libraryIds: string[];
  onChange: (libraryIds: string[]) => void;
}) {
  const { t } = useI18n();
  const { libraries, error } = props.knowledge;
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
  thinkingLevel: React.ReactNode;
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
        {props.thinkingLevel}
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
  thinkingLevel: React.ReactNode;
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
        {props.thinkingLevel}
        <span className="dedicated-wizard-actions-end">
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
        </span>
      </div>
    </>
  );
}

/** Thinking-level picker for drafting; hidden when the model declares nothing to choose.
 * 起草所用思考档位选择器；模型未声明可选档位时隐藏。 */
export function DedicatedThinkingLevelControl(props: {
  options: ComposerSelectOption[];
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  if (props.options.length < 2) return null;
  return (
    <Select disabled={props.disabled} onValueChange={props.onChange} value={props.value}>
      <SelectTrigger aria-label={t("composer.dedicated.thinkingLevel")} size="sm">
        <Brain size={14} />
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {props.options.map((option) => (
          <SelectItem key={option.value} value={option.value}>{reasoningLevelLabel(option.value, option.label, t)}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** Streamed thinking and draft text of the latest generation; collapses once generation ends.
 * 最近一次生成的流式思考与草稿文本；生成结束后折叠。 */
export function DedicatedThinkingPanel(props: { reasoning: string; output: string; streaming: boolean }) {
  const { t } = useI18n();
  const bodyRef = React.useRef<HTMLDivElement>(null);
  // Follow the newest text while streaming, as the conversation view does.
  // 流式输出期间跟随最新文本，与会话视图一致。
  React.useEffect(() => {
    const body = bodyRef.current;
    if (body && props.streaming) body.scrollTop = body.scrollHeight;
  }, [props.reasoning, props.output, props.streaming]);
  if (!props.streaming && props.reasoning.length === 0) return null;
  const waiting = props.reasoning.length === 0 && props.output.length === 0;
  return (
    <details className="dedicated-wizard-thinking" open={props.streaming || undefined}>
      <summary>
        {props.streaming ? <RefreshCw className="animate-spin" size={13} /> : <Brain size={13} />}
        {t(props.streaming ? "composer.dedicated.thinkingLive" : "composer.dedicated.thinking")}
      </summary>
      <div className="dedicated-wizard-thinking-body" ref={bodyRef}>
        {waiting ? <p>{t("composer.dedicated.thinkingWaiting")}</p> : null}
        {props.reasoning ? <p>{props.reasoning}</p> : null}
        {props.streaming && props.output ? (
          <>
            <strong>{t("composer.dedicated.draftPreview")}</strong>
            <p>{props.output}</p>
          </>
        ) : null}
      </div>
    </details>
  );
}
