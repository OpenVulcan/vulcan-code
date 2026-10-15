import * as React from "react";
import { Coins, HardDrive } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Combobox } from "@/components/ui/combobox";
import { useI18n } from "@/i18n";
import { loadPublicAiSettings } from "@/service/settings-actions-service";
import type { KnowledgeIndexMode, KnowledgeLibraryCreateRequest, KnowledgeLibrarySummary } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";
import { KNOWLEDGE_MODE_COPY, knowledgeEmbeddingCandidates, knowledgeRouteKey, knowledgeRouteFromCandidate, knowledgeSettingsFromDraft } from "./knowledge-mode";
import { KNOWLEDGE_MODE_ICONS } from "./knowledge-visuals";

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
  // Mode cards name themselves by label and describe themselves by their capability text.
  // 模式卡片以标签命名，并以能力说明作为描述。
  const cardId = React.useId();
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
    <DialogContent size="lg">
      <DialogHeader><DialogTitle>{t(library === null ? "knowledge.policy.createTitle" : "knowledge.policy.modeTitle")}</DialogTitle><DialogDescription>{t("knowledge.policy.description")}</DialogDescription></DialogHeader>
      <div className="knowledge-form">
        {library === null ? <>
          <label>{t("knowledge.name")}<Input aria-label={t("knowledge.name")} value={name} disabled={busy} onChange={(event) => setName(event.target.value)} /></label>
          <label>{t("knowledge.purpose")}<Input aria-label={t("knowledge.purpose")} placeholder={t("knowledge.metadataDescription")} value={description} disabled={busy} onChange={(event) => setDescription(event.target.value)} /></label>
        </> : <p className="font-medium">{library.name}</p>}
        <div className="knowledge-mode-grid" role="radiogroup" aria-label={t("knowledge.indexMode")}>
          {(Object.keys(KNOWLEDGE_MODE_COPY) as KnowledgeIndexMode[]).map((value) => {
            // Each card names its capability and whether it incurs background embedding calls.
            // 每张卡片说明其能力，以及是否产生后台嵌入调用。
            const Icon = KNOWLEDGE_MODE_ICONS[value];
            const costly = value === "vector" || value === "hybrid";
            return <button type="button" role="radio" aria-checked={mode === value} aria-labelledby={cardId + value + "-label"} aria-describedby={cardId + value + "-text"} className="knowledge-mode-card" data-mode={value} disabled={busy} key={value} onClick={() => setMode(value)}>
              <span><Icon size={16} aria-hidden="true" /></span>
              <strong id={cardId + value + "-label"}>{t(KNOWLEDGE_MODE_COPY[value].label)}</strong>
              <p id={cardId + value + "-text"}>{t(KNOWLEDGE_MODE_COPY[value].description)}</p>
              <small>{costly ? <Coins size={11} aria-hidden="true" /> : <HardDrive size={11} aria-hidden="true" />}{t(costly ? "knowledge.policy.costHint" : "knowledge.policy.localHint")}</small>
            </button>;
          })}
        </div>
        {mode === null ? <p className="knowledge-hint">{t("knowledge.policy.chooseMode")}</p> : null}
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
          <div className="knowledge-form-row">
            <label>{t("knowledge.policy.dimension")}<Input type="number" min={1} step={1} aria-label={t("knowledge.policy.dimension")} disabled={busy} value={dimension} onChange={(event) => setDimension(event.target.value)} /></label>
            <label>{t("knowledge.policy.batch")}<Input type="number" min={1} step={1} aria-label={t("knowledge.policy.batch")} disabled={busy} value={batch} onChange={(event) => setBatch(event.target.value)} /></label>
          </div>
          <p className="knowledge-hint">{t("knowledge.policy.serverBatch")}</p>
          {settings === null ? <p role="status" className="knowledge-hint">{t("knowledge.policy.dimensionRequired")}</p> : null}
        </> : null}
        {error !== null ? <pre role="alert" className="knowledge-error">{error}</pre> : null}
      </div>
      <DialogFooter><Button variant="outline" disabled={busy} onClick={onClose}>{t("conversation.action.cancel")}</Button><Button disabled={busy || settings === null || !name.trim()} onClick={() => void submit()}>{t(library === null ? "knowledge.create" : "memory.common.save")}</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
