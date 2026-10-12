import React from "react";
import { Brain, RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { reasoningLevelLabel } from "@/components/composer/model-reasoning-picker-control";
import { formatMessage, pickBilingualText, useI18n } from "@/i18n";
import type { AnswerMode } from "@/service/answer-mode";
import { listKnowledgeLibraries } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, OutputStyleCatalogSummary } from "@/types";
import { DEDICATED_RECOMMENDED_ANSWER_MODE, DEDICATED_RECOMMENDED_OUTPUT_STYLE_ID } from "./dedicated-draft-store";

/** One selectable wizard answer.
 * 单个可选向导答案。 */
export interface DedicatedWizardOption<T> {
  value: T;
  label: string;
  hint?: string;
  /** Short marker shown beside the label, such as the recommended choice.
   * 显示在标签旁的简短标记，例如推荐选项。 */
  badge?: string;
  /** Decorative sketch above the label that shows what the choice looks like.
   * 标签上方的装饰性示意图，直观展示该选项的效果。 */
  preview?: React.ReactNode;
  /** Relative size in [0, 1] drawn as a bar below the hint, so options compare at a glance.
   * 以提示下方横条绘制的 [0, 1] 相对大小，便于一眼比较各选项。 */
  meter?: number;
}

/** Renders mutually exclusive or multi-select answers as a grid of pressable cards with a choice mark.
 * 将互斥或多选答案渲染为带选择标记的可按压卡片矩阵。
 *
 * @param props.columns fixed column count for a small known set; narrow wizards halve it. Omitted, cards auto-fill.
 *                      已知少量选项的固定列数，较窄时减半；省略时卡片自动填充。
 * @param props.multiple marks the cards as checkboxes instead of radio choices. */
export function DedicatedWizardOptions<T>(props: {
  options: DedicatedWizardOption<T>[];
  isSelected: (value: T) => boolean;
  onSelect: (value: T) => void;
  disabled?: boolean;
  columns?: 2 | 3 | 4;
  multiple?: boolean;
}) {
  return (
    <div className="dedicated-wizard-options" data-columns={props.columns} data-multiple={props.multiple || undefined}>
      {props.options.map((option) => (
        <button
          aria-pressed={props.isSelected(option.value)}
          className="dedicated-wizard-option"
          disabled={props.disabled}
          key={String(option.value)}
          onClick={() => props.onSelect(option.value)}
          type="button"
        >
          {option.preview}
          <span className="dedicated-wizard-option-title">
            {option.label}
            {option.badge ? <em className="dedicated-wizard-option-badge">{option.badge}</em> : null}
            <span aria-hidden="true" className="dedicated-wizard-option-mark" />
          </span>
          {option.hint ? <small>{option.hint}</small> : null}
          {option.meter === undefined ? null : (
            <span aria-hidden="true" className="dedicated-wizard-option-meter">
              <span style={{ inlineSize: `${Math.round(Math.min(Math.max(option.meter, 0), 1) * 100)}%` }} />
            </span>
          )}
        </button>
      ))}
    </div>
  );
}

/** Miniature answer sketch: plain Markdown lines, or a heading over a chart and a card for interactive answers.
 * 回答缩略示意：标准回答为纯 Markdown 文本行，交互回答在标题下方显示图表与卡片。 */
function AnswerModePreview(props: { mode: AnswerMode }) {
  return (
    <span aria-hidden="true" className="dedicated-answer-preview" data-mode={props.mode}>
      <span className="dedicated-answer-preview-line" data-variant="heading" />
      {props.mode === "standard" ? (
        <>
          <span className="dedicated-answer-preview-line" />
          <span className="dedicated-answer-preview-line" />
          <span className="dedicated-answer-preview-line" data-variant="short" />
        </>
      ) : (
        <span className="dedicated-answer-preview-widgets">
          <span className="dedicated-answer-preview-chart"><i /><i /><i /><i /></span>
          <span className="dedicated-answer-preview-card"><i /><i /></span>
        </span>
      )}
    </span>
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
      multiple
      onSelect={(value) => {
        if (value === null) props.onChange([]);
        else if (props.libraryIds.includes(value)) props.onChange(props.libraryIds.filter((id) => id !== value));
        else props.onChange([...props.libraryIds, value]);
      }}
      options={options}
    />
  );
}

/** Basic-settings step: knowledge, answer mode and output style on one page; the recommended choices carry a badge.
 * 基础设置步骤：知识库、回答模式与输出风格同页设置；推荐选项带有标记。
 *
 * @param props.answerMode the effective answer mode (the wizard's pick, else the recommended mode).
 *                         生效的回答模式（向导选择，否则为推荐模式）。
 * @param props.styleId the effective output style (the wizard's pick, else the recommended style). */
export function DedicatedSettingsStep(props: {
  knowledge: ProjectKnowledgeLibraries;
  libraryIds: string[];
  onKnowledgeChange: (libraryIds: string[]) => void;
  answerMode: AnswerMode;
  onAnswerModeChange: (answerMode: AnswerMode) => void;
  catalog: OutputStyleCatalogSummary;
  styleId: string;
  onStyleChange: (styleId: string) => void;
}) {
  const { locale, t } = useI18n();
  const recommended = t("composer.dedicated.recommended");
  const badge = (isRecommended: boolean) => (isRecommended ? recommended : undefined);
  const answerModeOptions: DedicatedWizardOption<AnswerMode>[] = (["standard", "interactive"] as const).map((value) => ({
    value,
    label: t(value === "standard" ? "answerMode.standard" : "answerMode.interactive"),
    hint: t(value === "standard" ? "answerMode.standardHint" : "answerMode.interactiveHint"),
    badge: badge(value === DEDICATED_RECOMMENDED_ANSWER_MODE),
    preview: <AnswerModePreview mode={value} />,
  }));
  // Each style's bar is its prompt size against the largest one, showing how much a style adds to the assistant prompt.
  // 每个风格的横条为其提示词长度相对最长者的比例，表示该风格会给助手提示词增加多少内容。
  const largestStyle = Math.max(1, ...props.catalog.entries.map((entry) => entry.promptCharCount));
  const styleCount = props.catalog.entries.length;
  const styleColumns = styleCount >= 2 && styleCount <= 4 ? styleCount as 2 | 3 | 4 : undefined;
  const styleOptions: DedicatedWizardOption<string>[] = props.catalog.entries.map((entry) => ({
    value: entry.id,
    label: pickBilingualText(locale, entry.labelZh, entry.labelEn),
    hint: formatMessage(t("composer.outputStyle.promptCharacters"), { count: entry.promptCharCount.toLocaleString(locale) }),
    badge: badge(entry.id === DEDICATED_RECOMMENDED_OUTPUT_STYLE_ID),
    meter: entry.promptCharCount / largestStyle,
  }));
  return (
    <>
      {/* The section appears once libraries exist (or loading failed), so an empty project never flashes it. */}
      {/* 分区在确认存在知识库（或加载失败）后才出现，没有知识库的项目不会闪现该分区。 */}
      {!props.knowledge.libraries?.length && !props.knowledge.error ? null : (
        <section className="dedicated-wizard-section">
          <h3>{t("composer.dedicated.step.knowledge")}</h3>
          <DedicatedKnowledgeStep knowledge={props.knowledge} libraryIds={props.libraryIds} onChange={props.onKnowledgeChange} />
        </section>
      )}
      <section className="dedicated-wizard-section">
        <h3>{t("composer.dedicated.step.answerMode")}</h3>
        <DedicatedWizardOptions
          columns={2}
          isSelected={(value) => value === props.answerMode}
          onSelect={props.onAnswerModeChange}
          options={answerModeOptions}
        />
      </section>
      <section className="dedicated-wizard-section">
        <h3>{t("composer.dedicated.step.outputStyle")}</h3>
        <DedicatedWizardOptions
          columns={styleColumns}
          isSelected={(value) => value === props.styleId}
          onSelect={props.onStyleChange}
          options={styleOptions}
        />
      </section>
    </>
  );
}

/** Prompt step: picks the session model with its thinking level and takes the base prompt typed here; generation
 * happens in a separate dialog.
 * 提示词步骤：选择会话模型及其思考档位，并以此处填写的内容作为基础提示词；生成在独立弹窗中进行。
 *
 * @param props.modelControl the session model, account and thinking-level control owned by the wizard.
 *                           由向导持有的会话模型、账号与思考档位控件。 */
export function DedicatedPromptStep(props: {
  prompt: string;
  disabled: boolean;
  canGenerate: boolean;
  modelControl: React.ReactNode;
  onPromptChange: (prompt: string) => void;
  onOpenGenerator: () => void;
}) {
  const { t } = useI18n();
  return (
    <>
      <div className="dedicated-wizard-model">
        <span className="dedicated-wizard-hint">{t("composer.dedicated.sessionModel")}</span>
        {props.modelControl}
      </div>
      <Textarea
        aria-label={t("composer.dedicated.prompt")}
        className="dedicated-wizard-prompt"
        density="large"
        disabled={props.disabled}
        onChange={(event) => props.onPromptChange(event.target.value)}
        placeholder={t("composer.dedicated.promptPlaceholder")}
        value={props.prompt}
      />
      <div className="dedicated-wizard-actions">
        <span className="dedicated-wizard-hint">{t("composer.dedicated.promptImmutableHint")}</span>
        <Button
          disabled={props.disabled || !props.canGenerate}
          onClick={props.onOpenGenerator}
          size="sm"
          type="button"
          variant="outline"
        >
          <Sparkles size={14} />
          {t("composer.dedicated.generate")}
        </Button>
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
