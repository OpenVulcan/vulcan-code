import * as React from "react";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/ui/combobox";
import { useI18n } from "@/i18n";
import { isPublicAiReasoningEffort, PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS } from "@/lib/reasoning-effort-labels";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { PublicAiInternalInferenceCandidate } from "@/types/public-ai";
import type { KnowledgeRetrievalMode } from "@/types";

// KnowledgeLibraryCreateDialogModel describes the creation choices returned to the host page.
// KnowledgeLibraryCreateDialogModel 描述返回给宿主页面的创建选项。
export interface KnowledgeLibraryCreateDialogModel {
  // Retrieval policy selected by the user.
  // 用户选择的检索策略。
  retrievalMode: KnowledgeRetrievalMode;
  // Selected curation candidate, or null for raw BM25.
  // 选中的整理模型候选，原始 BM25 模式下为空。
  curationCandidate: PublicAiInternalInferenceCandidate | null;
  // Selected provider-neutral reasoning effort for the curation model.
  // 整理模型选中的供应商无关推理级别。
  reasoningEffort: string;
  // Selected embedding candidate, or null when vectors are disabled.
  // 选中的向量模型候选，关闭向量时为空。
  embeddingCandidate: PublicAiInternalInferenceCandidate | null;
  // Optional configured embedding dimension.
  // 可选的向量配置维度。
  configuredDimension: number | null;
  // Optional user batch override.
  // 可选的用户批量覆盖值。
  requestedBatchSize: number | null;
  // User-confirmed compatibility note for reusing a same-dimension index.
  // 用户确认复用同维度索引时填写的兼容性风险说明。
  compatibilityOverride: string;
}

// KnowledgeLibraryCreateDialogProps defines the controlled creation wizard boundary.
// KnowledgeLibraryCreateDialogProps 定义受控创建向导的边界。
export interface KnowledgeLibraryCreateDialogProps {
  // Whether the dialog is visible.
  // 对话框是否可见。
  open: boolean;
  // Parent-owned mutation state freezes creation and editing controls while saving.
  // 父组件持有的变更状态在保存期间冻结创建和编辑控件。
  busy?: boolean;
  // Dialog title shown for create or edit operations.
  // 创建或编辑操作显示的对话框标题。
  title?: string;
  // Submit button label.
  // 提交按钮文案。
  submitLabel?: string;
  // User-visible library name.
  // 用户可见资料库名称。
  name: string;
  // Candidate routes from the official capability snapshot.
  // 来自官方能力快照的候选路由。
  candidates: PublicAiInternalInferenceCandidate[];
  // Current controlled retrieval mode.
  // 当前受控检索模式。
  retrievalMode: KnowledgeRetrievalMode;
  // Current curation candidate key.
  // 当前整理候选键。
  curationCandidateKey: string;
  // Current curation reasoning effort.
  // 当前整理推理级别。
  reasoningEffort: string;
  // Current embedding candidate key.
  // 当前向量候选键。
  embeddingCandidateKey: string;
  // Configured dimension input.
  // 配置维度输入。
  configuredDimension: string;
  // Batch override input.
  // 批量覆盖输入。
  requestedBatchSize: string;
  // Compatibility note for an existing vector index.
  // 现有向量索引的兼容性风险说明。
  compatibilityOverride: string;
  // Controlled state callbacks.
  // 受控状态回调。
  onOpenChange: (open: boolean) => void;
  onNameChange: (name: string) => void;
  onRetrievalModeChange: (mode: KnowledgeRetrievalMode) => void;
  onCurationCandidateChange: (key: string) => void;
  onReasoningEffortChange: (value: string) => void;
  onEmbeddingCandidateChange: (key: string) => void;
  onConfiguredDimensionChange: (value: string) => void;
  onRequestedBatchSizeChange: (value: string) => void;
  // Updates the compatibility note.
  // 更新兼容性说明。
  onCompatibilityOverrideChange: (value: string) => void;
  onSubmit: (model: KnowledgeLibraryCreateDialogModel) => void;
}

// routeKey creates a stable UI key from the exact immutable route identity.
// routeKey 根据不可变路由身份创建稳定 UI 键。
function routeKey(candidate: PublicAiInternalInferenceCandidate): string {
  const route = candidate.route;
  return [route.provider_id, route.service_entry_id, route.connection_id, route.model_id, route.operation, route.binding.kind, route.binding.kind === "protocol" ? route.binding.protocol_profile_id : route.binding.action_binding_id].join("/");
}

// candidateLabel formats a route without exposing credentials.
// candidateLabel 格式化路由且不暴露凭据。
// Parameters: candidate is one exact knowledge-model route; returns a readable provider/model/account label without secrets.
// 参数：candidate 是一条精确知识库模型路由；返回不含秘密的供应商、模型和账号标签。
function candidateLabel(candidate: PublicAiInternalInferenceCandidate): string {
  return `${candidate.modelDisplayName} · ${candidate.providerDisplayName} · ${candidate.connectionDisplayName}`;
}

// knowledgeCurationCandidates returns enabled background LLM routes from the shared internal pool.
// knowledgeCurationCandidates 从共享内部模型池返回已启用的后台 LLM 路由。
export function knowledgeCurationCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "conversation_respond"
  ));
}

// knowledgeEmbeddingCandidates returns enabled background embedding routes with declared capabilities.
// knowledgeEmbeddingCandidates 返回已启用且声明向量能力的后台向量路由。
export function knowledgeEmbeddingCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "embedding_create"
    && Boolean(candidate.capabilities.embedding)
  ));
}

// KnowledgeLibraryCreateDialog renders creation-time policy and model selection.
// KnowledgeLibraryCreateDialog 渲染创建阶段的策略与模型选择。
// Parameters: controlled props supply name, retrieval/model policy drafts, busy state and field/submit callbacks; returns the validated create or edit form.
// 参数：受控属性提供名称、检索和模型策略草稿、忙碌状态及字段和提交回调；返回经过校验的创建或编辑表单。
export function KnowledgeLibraryCreateDialog({
  open,
  busy = false,
  title,
  submitLabel,
  name,
  candidates,
  retrievalMode,
  curationCandidateKey,
  reasoningEffort,
  embeddingCandidateKey,
  configuredDimension,
  requestedBatchSize,
  compatibilityOverride,
  onOpenChange,
  onNameChange,
  onRetrievalModeChange,
  onCurationCandidateChange,
  onReasoningEffortChange,
  onEmbeddingCandidateChange,
  onConfiguredDimensionChange,
  onRequestedBatchSizeChange,
  onCompatibilityOverrideChange,
  onSubmit,
}: KnowledgeLibraryCreateDialogProps) {
  // t localizes the shared model selection interaction without changing route identities.
  // t 本地化共享模型选择交互，不改变路由身份。
  const { t } = useI18n();
  const curationCandidates = knowledgeCurationCandidates(candidates);
  const embeddingCandidates = knowledgeEmbeddingCandidates(candidates);
  const requiresCuration = retrievalMode !== "raw_bm25";
  const requiresEmbedding = retrievalMode === "display_hybrid";
  const curationCandidate = curationCandidates.find((candidate) => routeKey(candidate) === curationCandidateKey) ?? null;
  const reasoningEfforts = React.useMemo(
    () => Array.from(new Set(curationCandidate?.capabilities.reasoning_efforts ?? [])),
    [curationCandidate],
  );
  React.useEffect(() => {
    if (!open || !requiresCuration || reasoningEfforts.length === 0 || reasoningEffort) return;
    onReasoningEffortChange(reasoningEfforts[0]);
  }, [onReasoningEffortChange, open, reasoningEffort, reasoningEfforts, requiresCuration]);
  const embeddingCandidate = embeddingCandidates.find((candidate) => routeKey(candidate) === embeddingCandidateKey) ?? null;
  const embeddingCapabilities = embeddingCandidate?.capabilities.embedding;
  const dimensions = React.useMemo(() => {
    if (!embeddingCapabilities) return [];
    // Only an explicit discrete list constrains the selector; default_dimensions is a recommendation.
    // 只有官方明确声明的离散列表才限制选择器；default_dimensions 只是推荐值。
    const declared = embeddingCapabilities.dimensions.length > 0
      ? embeddingCapabilities.dimensions
      : [];
    return Array.from(new Set(declared)).sort((left, right) => left - right);
  }, [embeddingCapabilities]);
  const maximumInputs = embeddingCapabilities?.maximum_inputs;
  const configuredDimensionNumber = configuredDimension.trim() ? Number(configuredDimension) : null;
  // Retained numeric overrides still cross Option<u32> even when the user turns vector retrieval off.
  // 即使用户关闭向量检索，保留的数值覆盖仍需经过 Option<u32> 契约。
  const dimensionRepresentable = configuredDimensionNumber === null || (Number.isSafeInteger(configuredDimensionNumber) && configuredDimensionNumber > 0 && configuredDimensionNumber <= 4_294_967_295);
  const dimensionValid = embeddingCapabilities
    ? configuredDimensionNumber !== null
      && Number.isSafeInteger(configuredDimensionNumber)
      && configuredDimensionNumber > 0
      && dimensionRepresentable
      && (dimensions.length === 0 || dimensions.includes(configuredDimensionNumber))
      && (embeddingCapabilities.minimum_dimensions === undefined || configuredDimensionNumber >= embeddingCapabilities.minimum_dimensions)
      && (embeddingCapabilities.maximum_dimensions === undefined || configuredDimensionNumber <= embeddingCapabilities.maximum_dimensions)
    : false;
  // Batch validation mirrors Option<u32> and resolve_embedding_batch_size without replacing invalid text.
  // 批量校验对齐 Option<u32> 和 resolve_embedding_batch_size，不替换无效文本。
  const batchNumber = requestedBatchSize.trim() ? Number(requestedBatchSize) : null;
  const batchValid = batchNumber === null || (Number.isSafeInteger(batchNumber) && batchNumber > 0 && batchNumber <= 4_294_967_295 && (maximumInputs === undefined || batchNumber <= maximumInputs));
  const reasoningValid = !requiresCuration || reasoningEfforts.length === 0 || reasoningEfforts.includes(reasoningEffort);
  return (
    <Dialog onOpenChange={(nextOpen) => { if (!busy) onOpenChange(nextOpen) }} open={open}>
      <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden p-0">
        <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12"><DialogTitle>{title ?? t("knowledge.experience.createTitle")}</DialogTitle></DialogHeader>
        <fieldset disabled={busy} className="min-h-0 min-w-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
          <Input aria-label={t("knowledge.name")} onChange={(event) => onNameChange(event.target.value)} placeholder={t("knowledge.name")} value={name} />
          <label className="space-y-1"><span>{t("knowledge.experience.retrievalMode")}</span><Select onValueChange={(value) => onRetrievalModeChange(value as KnowledgeRetrievalMode)} value={retrievalMode}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="raw_bm25">{t("knowledge.experience.rawMode")}</SelectItem>
              <SelectItem value="display_bm25">{t("knowledge.experience.displayMode")}</SelectItem>
              <SelectItem value="display_hybrid">{t("knowledge.experience.hybridMode")}</SelectItem>
            </SelectContent>
          </Select></label>
          {requiresCuration ? <div className="space-y-1"><span>{t("settings.modelExperience.curationModel")}</span><Combobox
            ariaLabel={t("settings.modelExperience.curationModel")}
            emptyText={t("settings.modelExperience.curationNoMatch")}
            searchPlaceholder={t("settings.modelExperience.modelSearch")}
            placeholder={t(curationCandidateKey && !curationCandidate ? "settings.modelExperience.curationUnavailable" : curationCandidates.length ? "settings.modelExperience.chooseCuration" : "settings.modelExperience.noCuration")}
            onValueChange={onCurationCandidateChange}
            value={curationCandidateKey}
            options={curationCandidates.map((candidate) => ({
              value: routeKey(candidate), label: candidateLabel(candidate),
              description: `${candidate.serviceEntryDisplayName} · ${candidate.route.model_id}`,
              keywords: [candidate.route.provider_id, candidate.route.connection_id, candidate.route.model_id],
            }))}
          /></div> : null}
          {requiresCuration && reasoningEfforts.length > 0 ? <label className="space-y-1"><span>{t("knowledge.experience.reasoning")}</span><Select onValueChange={onReasoningEffortChange} value={reasoningEffort || undefined}>
            <SelectTrigger><SelectValue placeholder={t("knowledge.experience.reasoningPlaceholder")} /></SelectTrigger>
            <SelectContent>{reasoningEffort && !reasoningValid && <SelectItem disabled value={reasoningEffort}>{t("settings.modelExperience.invalidValue")}: {reasoningEffort}</SelectItem>}{reasoningEfforts.map((effort) => <SelectItem key={effort} value={effort}>{isPublicAiReasoningEffort(effort) ? t(PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS[effort]) : effort}</SelectItem>)}</SelectContent>
          </Select><span className="text-sm text-muted-foreground">{t("knowledge.experience.reasoningHint")}</span></label> : null}
          {requiresEmbedding ? <>
            <div className="space-y-1"><span>{t("settings.modelExperience.embeddingModel")}</span><Combobox
              ariaLabel={t("settings.modelExperience.embeddingModel")}
              emptyText={t("settings.modelExperience.embeddingNoMatch")}
              searchPlaceholder={t("settings.modelExperience.modelSearch")}
              placeholder={t(embeddingCandidateKey && !embeddingCandidate ? "settings.modelExperience.embeddingUnavailable" : embeddingCandidates.length ? "settings.modelExperience.chooseEmbedding" : "settings.modelExperience.noEmbedding")}
              onValueChange={onEmbeddingCandidateChange}
              value={embeddingCandidateKey}
              options={embeddingCandidates.map((candidate) => ({
                value: routeKey(candidate), label: candidateLabel(candidate),
                description: `${candidate.serviceEntryDisplayName} · ${candidate.route.model_id}`,
                keywords: [candidate.route.provider_id, candidate.route.connection_id, candidate.route.model_id],
              }))}
            /></div>
            <label className="space-y-1"><span>{t("knowledge.experience.dimensions")}</span>{dimensions.length > 0 ? <Select onValueChange={onConfiguredDimensionChange} value={configuredDimension}>
              <SelectTrigger><SelectValue placeholder={t("knowledge.experience.dimensionsPlaceholder")} /></SelectTrigger>
              <SelectContent>{dimensions.map((dimension) => <SelectItem key={dimension} value={String(dimension)}>{dimension}</SelectItem>)}</SelectContent>
            </Select> : <Input inputMode="numeric" min={embeddingCapabilities?.minimum_dimensions} max={embeddingCapabilities?.maximum_dimensions} onChange={(event) => onConfiguredDimensionChange(event.target.value)} placeholder={embeddingCapabilities?.default_dimensions ? String(embeddingCapabilities.default_dimensions) : t("knowledge.experience.dimensionsInput")} type="number" value={configuredDimension} />}{configuredDimension && !dimensionValid ? <span className="text-sm text-destructive">{t("knowledge.experience.dimensionsInvalid")}</span> : null}</label>
            <label className="space-y-1"><span>{t("knowledge.experience.batch").replace("{limit}", maximumInputs === undefined ? t("knowledge.experience.batchFallback") : String(maximumInputs))}</span><Input aria-invalid={!batchValid} inputMode="numeric" min={1} max={maximumInputs ?? 4_294_967_295} onChange={(event) => onRequestedBatchSizeChange(event.target.value)} placeholder={maximumInputs ? String(maximumInputs) : "10"} type="number" value={requestedBatchSize} />{!batchValid && <span role="alert" className="text-sm text-destructive">{t("settings.modelExperience.invalidValue")}</span>}</label>
            <label className="space-y-1"><span>{t("knowledge.experience.compatibility")}</span><Input onChange={(event) => onCompatibilityOverrideChange(event.target.value)} placeholder={t("knowledge.experience.compatibilityPlaceholder")} value={compatibilityOverride} /><span className="text-sm text-muted-foreground">{t("knowledge.experience.compatibilityHint")}</span></label>
          </> : null}
          {!requiresEmbedding && (!dimensionRepresentable || !batchValid) ? <p role="alert" className="text-sm text-destructive">{t("settings.modelExperience.retainedEmbeddingInvalid")}</p> : null}
        </fieldset>
        <DialogFooter className="shrink-0 px-5 py-3">
          <Button disabled={busy} onClick={() => onOpenChange(false)} variant="outline">{t("conversation.action.cancel")}</Button>
          <Button disabled={busy || !name.trim() || !reasoningValid || !dimensionRepresentable || !batchValid || (requiresCuration && !curationCandidate) || (requiresEmbedding && (!embeddingCandidate || !dimensionValid))} onClick={() => onSubmit({ retrievalMode, curationCandidate, reasoningEffort, embeddingCandidate, configuredDimension: configuredDimensionNumber, requestedBatchSize: batchNumber, compatibilityOverride: compatibilityOverride.trim() })}>{submitLabel ?? t("knowledge.create")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
