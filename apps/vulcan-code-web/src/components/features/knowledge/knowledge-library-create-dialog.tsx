import * as React from "react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Combobox } from "@/components/ui/combobox";
import { useI18n } from "@/i18n";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type { KnowledgeIndexMode, KnowledgeLibraryCreateRequest, KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";
import { KNOWLEDGE_MODE_COPY, knowledgeEmbeddingCandidates, knowledgeRouteKey, knowledgeRouteFromCandidate, knowledgeSettingsFromDraft } from "./knowledge-mode";

// One mounted dialog owns either a new library draft or an exact existing library mode change.
// 一个已挂载对话框拥有新库草稿或精确既有库模式变更。
export interface KnowledgeLibraryCreateDialogProps {
  // Existing immutable owner; null selects creation.
  // 既有不可变所有者；空值选择创建。
  library: KnowledgeLibrarySummary | null;
  // Blocks fields while the parent owns a mutation.
  // 父组件拥有变更时阻止字段操作。
  busy: boolean;
  // Dismisses this exact mounted draft.
  // 关闭此精确挂载草稿。
  onClose: () => void;
  // Receives explicit human name, purpose and settings; resolves only after committed save and refresh.
  // 接收显式人工名称、用途及设置；仅在提交保存及刷新后完成。
  onSubmit: (request: KnowledgeLibraryCreateRequest) => Promise<void>;
}

// Renders explicit creation/mode choices and official model admission without content-editing controls.
// 渲染显式创建及模式选择与官方模型准入，不提供内容编辑控件。
// Parameters: props freezes the exact owner and mutation callbacks. Returns an accessible controlled dialog.
// 参数：props 冻结精确所有者及变更回调。返回无障碍受控弹窗。
export function KnowledgeLibraryCreateDialog({ library, busy, onClose, onSubmit }: KnowledgeLibraryCreateDialogProps) {
  // Labels share the application's current language.
  // 标签共享应用当前语言。
  const { t } = useI18n();
  // Existing embedding fields belong only to this frozen dialog owner.
  // 既有嵌入字段仅属于此冻结对话框所有者。
  const initialEmbedding = library === null || library.settings === null ? null : library.settings.embedding;
  // New libraries require a conscious mode selection; existing modes are read from their sole settings owner.
  // 新库要求主动选择模式；既有模式从其唯一设置所有者读取。
  const [mode, setMode] = React.useState<KnowledgeIndexMode | null>(library === null || library.settings === null ? null : library.settings.indexMode);
  // Name is editable only during creation.
  // 名称仅在创建期间可编辑。
  const [name, setName] = React.useState(library === null ? "" : library.name);
  // Purpose guides the AI's conscious content maintenance.
  // 用途指导 AI 主动维护内容。
  const [description, setDescription] = React.useState(library === null ? "" : library.description);
  // The selected model has one exact route identity; no first-option default is chosen.
  // 选定模型具有一个精确路由身份；不选择首个选项作为默认值。
  const [routeKey, setRouteKey] = React.useState(() => initialEmbedding === null || initialEmbedding.route.connectionId === null ? "" : knowledgeRouteKey({
    providerId: initialEmbedding.route.providerId, serviceEntryId: initialEmbedding.route.serviceEntryId,
    connectionId: initialEmbedding.route.connectionId, modelId: initialEmbedding.route.modelId,
    bindingKind: initialEmbedding.route.bindingKind, protocol: initialEmbedding.route.protocol,
  }));
  // Output size is explicit, with only declared model defaults eligible for prefilling.
  // 输出大小明确，仅声明模型默认值可预填。
  const [dimension, setDimension] = React.useState(initialEmbedding === null ? "" : String(initialEmbedding.dimensions));
  // Retain a saved batch unless the human selects another route or explicitly clears it.
  // 保留已保存批量，除非人工选择其他路由或明确清空。
  const [batch, setBatch] = React.useState(initialEmbedding === null ? "" : String(initialEmbedding.batchSize));
  // Failed saves retain the exact draft for correction.
  // 保存失败保留精确草稿以便修正。
  const [error, setError] = React.useState<string | null>(null);
  // This ref closes the double-click window before React publishes the parent busy state.
  // 此引用在 React 发布父级忙碌状态前封闭双击窗口。
  const saving = React.useRef(false);
  // The shared settings endpoint is the sole candidate producer.
  // 共享设置端点为唯一候选产出者。
  const load = React.useCallback(loadPublicAiSettings, []);
  // Candidate reads keep failures distinct from an empty valid catalog.
  // 候选读取将失败与有效空目录区分。
  const models = useKnowledgeRead("knowledge-models", load);
  // Declared embedding membership narrows the model's optional capability.
  // 声明嵌入成员关系收窄模型可选能力。
  const candidates = knowledgeEmbeddingCandidates(models.value === null ? [] : models.value.internalInferenceCandidates);
  // Match only the chosen route, never a sibling model with a similar display name.
  // 仅匹配选定路由，绝非展示名称相近的同级模型。
  const selected = candidates.find((candidate) => knowledgeRouteKey(knowledgeRouteFromCandidate(candidate)) === routeKey);
  // Vector modes alone require model controls and inference cost disclosure.
  // 仅向量模式需要模型控件及推理成本说明。
  const vector = mode === "vector" || mode === "hybrid";
  // Null represents an unfinished or invalid draft, never a persisted fallback.
  // 空值代表未完成或非法草稿，绝非持久兜底。
  const settings = knowledgeSettingsFromDraft(mode, selected, dimension, batch);

  // Saves one ready draft and preserves failures; returns after the exact caller transaction completes.
  // 保存一个就绪草稿并保留失败；精确调用方事务完成后返回。
  async function submit(): Promise<void> {
    if (busy || saving.current || settings === null || name.trim() === "") return;
    saving.current = true;
    setError(null);
    try { await onSubmit({ name: name.trim(), description, settings, projectIds: [] }); onClose(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { saving.current = false; }
  }

  return <Dialog open onOpenChange={(open) => { if (!open && !busy && !saving.current) onClose(); }}>
    <DialogContent>
      <DialogHeader><DialogTitle>{t(library === null ? "knowledge.policy.createTitle" : "knowledge.policy.modeTitle")}</DialogTitle><DialogDescription>{t("knowledge.policy.description")}</DialogDescription></DialogHeader>
      <div className="space-y-4">
        {library === null ? <>
          <label className="grid gap-1">{t("knowledge.name")}<Input aria-label={t("knowledge.name")} value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
          <label className="grid gap-1">{t("knowledge.purpose")}<Input aria-label={t("knowledge.purpose")} value={description} disabled={busy} onChange={(event) => setDescription(event.target.value)} /></label>
        </> : <p className="font-medium">{library.name}</p>}
        <label className="grid gap-1">{t("knowledge.indexMode")}
          <Select value={mode === null ? "" : mode} disabled={busy} onValueChange={(value: KnowledgeIndexMode) => setMode(value)}>
            <SelectTrigger aria-label={t("knowledge.indexMode")}><SelectValue placeholder={t("knowledge.policy.chooseMode")} /></SelectTrigger>
            <SelectContent>{(Object.keys(KNOWLEDGE_MODE_COPY) as KnowledgeIndexMode[]).map((value) => <SelectItem key={value} value={value}>{t(KNOWLEDGE_MODE_COPY[value].label)}</SelectItem>)}</SelectContent>
          </Select>
        </label>
        {mode !== null ? <p className="text-sm text-muted-foreground">{t(KNOWLEDGE_MODE_COPY[mode].description)}</p> : null}
        {vector ? <>
          {models.error !== null ? <KnowledgeReadFailure error={models.error} onRetry={models.refresh} busy={models.loading} /> : null}
          <Combobox value={routeKey} disabled={busy || models.loading} ariaLabel={t("knowledge.policy.embedding")}
            placeholder={t("knowledge.policy.chooseModel")} searchPlaceholder={t("knowledge.policy.chooseModel")} emptyText={t("knowledge.policy.noModels")}
            options={candidates.map((candidate) => ({ value: knowledgeRouteKey(knowledgeRouteFromCandidate(candidate)), label: candidate.modelDisplayName + " · " + candidate.providerDisplayName + " · " + candidate.connectionDisplayName }))}
            onValueChange={(value) => {
              setRouteKey(value);
              // Only the exact selected candidate may prefill its declared default output dimension.
              // 仅精确选定候选可预填其声明默认输出维度。
              const candidate = candidates.find((entry) => knowledgeRouteKey(knowledgeRouteFromCandidate(entry)) === value);
              setDimension(candidate !== undefined && candidate.capabilities.embedding.default_dimensions !== undefined ? String(candidate.capabilities.embedding.default_dimensions) : "");
              setBatch("");
            }} />
          <label className="grid gap-1">{t("knowledge.policy.dimension")}<Input type="number" min={1} step={1} aria-label={t("knowledge.policy.dimension")} disabled={busy} value={dimension} onChange={(event) => setDimension(event.target.value)} /></label>
          <label className="grid gap-1">{t("knowledge.policy.batch")}<Input type="number" min={1} step={1} aria-label={t("knowledge.policy.batch")} disabled={busy} value={batch} onChange={(event) => setBatch(event.target.value)} /></label>
          <p className="text-sm text-muted-foreground">{t("knowledge.policy.serverBatch")}</p>
          {settings === null ? <p role="status" className="text-sm text-muted-foreground">{t("knowledge.policy.dimensionRequired")}</p> : null}
        </> : null}
        {error !== null ? <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive">{error}</pre> : null}
      </div>
      <DialogFooter><Button variant="outline" disabled={busy} onClick={onClose}>{t("conversation.action.cancel")}</Button><Button disabled={busy || settings === null || !name.trim()} onClick={() => void submit()}>{t(library === null ? "knowledge.create" : "memory.common.save")}</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
