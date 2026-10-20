import React from "react";
import { RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { modelPickerChoicesFromProviders } from "@/components/composer/model-picker-control";
import { useI18n } from "@/i18n";
import { draftDedicatedPrompt, subscribeDedicatedPromptDraftProgress } from "@/service/dedicated-session-service";
import type { ProviderSummary } from "@/types";
import { dedicatedErrorMessage } from "./dedicated-errors";
import { DedicatedThinkingLevelControl, DedicatedThinkingPanel } from "./dedicated-wizard-steps";

/** Separate dialog that drafts or revises the base prompt; nothing reaches the wizard until the user applies it.
 * 起草或优化基础提示词的独立弹窗；用户写入之前不会改动向导中的提示词。
 *
 * @param props.currentPrompt the wizard's prompt, used as the starting draft each time the dialog opens.
 *                            向导中的提示词，每次打开弹窗时作为起始草稿。
 * @param props.requirement the stored description; props.onRequirementChange keeps it across openings.
 *                          已保存的描述；props.onRequirementChange 使其跨次打开保留。
 * @param props.providers supply the session model chosen on the prompt step, which drafting uses as is.
 *                        提供提示词步骤所选的会话模型，起草按原样使用该模型。
 * @param props.onApply receives the reviewed draft and closes the dialog. */
export function DedicatedPromptGeneratorDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providers: ProviderSummary[];
  currentPrompt: string;
  requirement: string;
  onRequirementChange: (requirement: string) => void;
  onApply: (prompt: string) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = React.useState("");
  const [revision, setRevision] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [thinking, setThinking] = React.useState({ reasoning: "", output: "" });
  // Each opening starts from the wizard's current prompt so an existing prompt can be revised.
  // 每次打开都从向导当前提示词开始，以便优化已有提示词。
  const { currentPrompt, open } = props;
  React.useEffect(() => {
    if (!open) return;
    setDraft(currentPrompt);
    setRevision("");
    setError(null);
    setThinking({ reasoning: "", output: "" });
  }, [currentPrompt, open]);
  // activeChoice is the exact route the composer would use; drafting never substitutes it.
  // activeChoice 是输入区将使用的精确路由；起草从不替换它。
  const activeChoice = React.useMemo(
    () => modelPickerChoicesFromProviders(props.providers).find((choice) => choice.active) ?? null,
    [props.providers],
  );
  // levelOptions are the thinking levels the exact drafting route declares.
  // levelOptions 为精确起草路由声明的思考档位。
  const reasoning = React.useMemo(() => {
    if (!activeChoice) return null;
    const provider = props.providers.find((candidate) => candidate.connectionId === activeChoice.connectionId);
    return provider?.models?.find((model) => model.id === activeChoice.modelId)?.reasoningSelection ?? null;
  }, [activeChoice, props.providers]);
  const levelOptions = React.useMemo(() => reasoning?.options ?? [], [reasoning]);
  const [chosenLevel, setChosenLevel] = React.useState<string | null>(null);
  // level is the user's choice when the model still declares it, else the model's own default level.
  // level 为用户的选择（模型仍声明该档位时），否则为模型自身的默认档位。
  const declared = (value: string | null | undefined): value is string =>
    Boolean(value) && levelOptions.some((option) => option.value === value);
  const level = [chosenLevel, reasoning?.defaultEffort, reasoning?.selectedEffort].find(declared)
    ?? levelOptions[0]?.value ?? "";
  // submittedLevel is sent only for an adjustable scale; fixed states let the service resolve the model default.
  // submittedLevel 仅在档位可调时发送；固定状态交由服务解析模型默认值。
  const submittedLevel = levelOptions.length >= 2 && level !== "fixed-on" && level !== "fixed-off" ? level : undefined;

  const generate = async (revise: boolean) => {
    if (!activeChoice || busy) return;
    setBusy(true);
    setError(null);
    setThinking({ reasoning: "", output: "" });
    const progressId = crypto.randomUUID();
    let unsubscribe: (() => void) | null = null;
    try {
      unsubscribe = await subscribeDedicatedPromptDraftProgress((event) => {
        if (event.progressId !== progressId) return;
        setThinking((current) => ({
          reasoning: current.reasoning + event.reasoningDelta,
          output: current.output + event.outputDelta,
        }));
      });
      const response = await draftDedicatedPrompt({
        providerId: activeChoice.providerId,
        connectionId: activeChoice.connectionId,
        modelId: activeChoice.modelId,
        requirement: props.requirement,
        currentPrompt: revise ? draft : undefined,
        revisionRequest: revise ? revision : undefined,
        reasoningEffort: submittedLevel,
        progressId,
      });
      setDraft(response.prompt);
      setRevision("");
    } catch (reason) {
      setError(dedicatedErrorMessage(reason, t));
    } finally {
      unsubscribe?.();
      setBusy(false);
    }
  };

  const busyIcon = (icon: React.ReactNode) => (busy ? <RefreshCw className="vk-spin" size={14} /> : icon);
  return (
    <Dialog onOpenChange={(next) => { if (!busy) props.onOpenChange(next); }} open={props.open}>
      <DialogContent aria-busy={busy} scrollMode="body-scroll" size="lg">
        <DialogHeader>
          <DialogTitle>{t("composer.dedicated.generateTitle")}</DialogTitle>
        </DialogHeader>
        <div className="vk-gen">
          <label className="vk-field">
            <span className="vk-field-label">{t("composer.dedicated.requirement")}</span>
            <Textarea
              density="large"
              disabled={busy}
              onChange={(event) => props.onRequirementChange(event.target.value)}
              placeholder={t("composer.dedicated.requirementPlaceholder")}
              value={props.requirement}
            />
          </label>
          <div className="vk-wiz-bar">
            <span className="vk-wiz-hint">{t("composer.dedicated.generatorModel")}</span>
            <span className="vk-gen-model">{activeChoice?.modelName ?? t("composer.modelPicker.placeholder")}</span>
            <DedicatedThinkingLevelControl disabled={busy} onChange={setChosenLevel} options={levelOptions} value={level} />
            <span className="vk-wiz-end">
              <Button
                disabled={busy || !activeChoice || props.requirement.trim().length === 0}
                onClick={() => void generate(false)}
                size="sm"
                type="button"
              >
                {busyIcon(<Sparkles size={14} />)}
                {t("composer.dedicated.generate")}
              </Button>
            </span>
          </div>
          <DedicatedThinkingPanel output={thinking.output} reasoning={thinking.reasoning} streaming={busy} />
          {draft.trim() ? (
            <>
              <label className="vk-field">
                <span className="vk-field-label">{t("composer.dedicated.draftPreview")}</span>
                <Textarea density="large" disabled={busy} onChange={(event) => setDraft(event.target.value)} value={draft} />
              </label>
              <div className="vk-gen-revision">
                <Textarea
                  aria-label={t("composer.dedicated.revision")}
                  density="compact"
                  disabled={busy}
                  onChange={(event) => setRevision(event.target.value)}
                  placeholder={t("composer.dedicated.revisionPlaceholder")}
                  value={revision}
                />
                <Button
                  disabled={busy || !activeChoice || revision.trim().length === 0}
                  onClick={() => void generate(true)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {busyIcon(<Sparkles size={14} />)}
                  {t("composer.dedicated.revise")}
                </Button>
              </div>
            </>
          ) : null}
          {error ? <p className="vk-wiz-error" role="alert">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button disabled={busy} onClick={() => props.onOpenChange(false)} type="button" variant="ghost">
            {t("composer.dedicated.cancel")}
          </Button>
          <Button disabled={busy || draft.trim().length === 0} onClick={() => props.onApply(draft)} type="button">
            {t("composer.dedicated.apply")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
