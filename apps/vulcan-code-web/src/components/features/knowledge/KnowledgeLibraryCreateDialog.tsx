import * as React from "react";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
function candidateLabel(candidate: PublicAiInternalInferenceCandidate): string {
  return `${candidate.providerDisplayName} · ${candidate.modelDisplayName}`;
}

// KnowledgeLibraryCreateDialog renders creation-time policy and model selection.
// KnowledgeLibraryCreateDialog 渲染创建阶段的策略与模型选择。
export function KnowledgeLibraryCreateDialog({
  open,
  title = "创建资料库",
  submitLabel = "创建",
  name,
  candidates,
  retrievalMode,
  curationCandidateKey,
  embeddingCandidateKey,
  configuredDimension,
  requestedBatchSize,
  compatibilityOverride,
  onOpenChange,
  onNameChange,
  onRetrievalModeChange,
  onCurationCandidateChange,
  onEmbeddingCandidateChange,
  onConfiguredDimensionChange,
  onRequestedBatchSizeChange,
  onCompatibilityOverrideChange,
  onSubmit,
}: KnowledgeLibraryCreateDialogProps) {
  const curationCandidates = candidates.filter((candidate) => candidate.enabled && candidate.route.usage_scope === "internal_background" && candidate.route.operation === "conversation_respond");
  const embeddingCandidates = candidates.filter((candidate) => candidate.enabled && candidate.route.usage_scope === "internal_background" && candidate.route.operation === "embedding_create" && candidate.capabilities.embedding);
  const curationCandidate = curationCandidates.find((candidate) => routeKey(candidate) === curationCandidateKey) ?? null;
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
  const dimensionValid = embeddingCapabilities
    ? configuredDimensionNumber !== null
      && Number.isInteger(configuredDimensionNumber)
      && configuredDimensionNumber > 0
      && (dimensions.length === 0 || dimensions.includes(configuredDimensionNumber))
      && (embeddingCapabilities.minimum_dimensions === undefined || configuredDimensionNumber >= embeddingCapabilities.minimum_dimensions)
      && (embeddingCapabilities.maximum_dimensions === undefined || configuredDimensionNumber <= embeddingCapabilities.maximum_dimensions)
    : false;
  const requiresCuration = retrievalMode !== "raw_bm25";
  const requiresEmbedding = retrievalMode === "display_hybrid";

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent>
        <DialogHeader><DialogTitle>{title}</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <Input aria-label="资料库名称" onChange={(event) => onNameChange(event.target.value)} placeholder="资料库名称" value={name} />
          <label className="space-y-1"><span>检索模式</span><Select onValueChange={(value) => onRetrievalModeChange(value as KnowledgeRetrievalMode)} value={retrievalMode}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="raw_bm25">原始库 BM25（无模型成本）</SelectItem>
              <SelectItem value="display_bm25">LLM 整理显示库 + BM25</SelectItem>
              <SelectItem value="display_hybrid">LLM 整理显示库 + BM25 + 向量</SelectItem>
            </SelectContent>
          </Select></label>
          {requiresCuration ? <label className="space-y-1"><span>整理模型</span><Select onValueChange={onCurationCandidateChange} value={curationCandidateKey}>
            <SelectTrigger><SelectValue placeholder={curationCandidates.length ? "选择整理模型" : "没有可用的整理模型"} /></SelectTrigger>
            <SelectContent>{curationCandidates.map((candidate) => <SelectItem key={routeKey(candidate)} value={routeKey(candidate)}>{candidateLabel(candidate)}</SelectItem>)}</SelectContent>
          </Select></label> : null}
          {requiresEmbedding ? <>
            <label className="space-y-1"><span>向量模型</span><Select onValueChange={onEmbeddingCandidateChange} value={embeddingCandidateKey}>
              <SelectTrigger><SelectValue placeholder={embeddingCandidates.length ? "选择向量模型" : "没有可用的向量模型"} /></SelectTrigger>
              <SelectContent>{embeddingCandidates.map((candidate) => <SelectItem key={routeKey(candidate)} value={routeKey(candidate)}>{candidateLabel(candidate)}</SelectItem>)}</SelectContent>
            </Select></label>
            <label className="space-y-1"><span>向量维度</span>{dimensions.length > 0 ? <Select onValueChange={onConfiguredDimensionChange} value={configuredDimension}>
              <SelectTrigger><SelectValue placeholder="选择模型声明的维度" /></SelectTrigger>
              <SelectContent>{dimensions.map((dimension) => <SelectItem key={dimension} value={String(dimension)}>{dimension}</SelectItem>)}</SelectContent>
            </Select> : <Input inputMode="numeric" min={embeddingCapabilities?.minimum_dimensions} max={embeddingCapabilities?.maximum_dimensions} onChange={(event) => onConfiguredDimensionChange(event.target.value)} placeholder={embeddingCapabilities?.default_dimensions ? String(embeddingCapabilities.default_dimensions) : "输入模型支持的维度"} type="number" value={configuredDimension} />}{configuredDimension && !dimensionValid ? <span className="text-sm text-destructive">维度不在当前模型能力范围内</span> : null}</label>
            <label className="space-y-1"><span>Embedding 批量（官方上限：{maximumInputs ?? "未声明，回退 10"}）</span><Input inputMode="numeric" min={1} onChange={(event) => onRequestedBatchSizeChange(event.target.value)} placeholder={maximumInputs ? String(maximumInputs) : "10"} type="number" value={requestedBatchSize} /></label>
            <label className="space-y-1"><span>同维度兼容性确认（可选）</span><Input onChange={(event) => onCompatibilityOverrideChange(event.target.value)} placeholder="模型更换但维度相同，确认可复用现有索引时填写原因" value={compatibilityOverride} /><span className="text-sm text-muted-foreground">填写后会保留旧索引并显示风险；维度不一致时仍会自动关闭向量检索。</span></label>
          </> : null}
        </div>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} variant="outline">取消</Button>
          <Button disabled={!name.trim() || (requiresCuration && !curationCandidate) || (requiresEmbedding && (!embeddingCandidate || !dimensionValid))} onClick={() => onSubmit({ retrievalMode, curationCandidate, embeddingCandidate, configuredDimension: configuredDimensionNumber, requestedBatchSize: requestedBatchSize ? Number(requestedBatchSize) : null, compatibilityOverride: compatibilityOverride.trim() })}>{submitLabel}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
