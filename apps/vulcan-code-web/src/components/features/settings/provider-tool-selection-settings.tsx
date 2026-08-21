import * as React from "react";
import { FlaskConical } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import { isModelExtensionCapability } from "@/lib/provider-tool-capabilities";
import {
  loadPublicAiSettings,
  savePublicAiMediaUnderstandingSelection,
  savePublicAiProviderExtensionTargetSet,
  savePublicAiProviderToolCapabilityEnabled,
  savePublicAiProviderToolSelection,
} from "@/service/settings-actions-service";
import type {
  PublicAiProviderToolCapabilityKind,
  PublicAiMediaUnderstandingBackendOptions,
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiMediaUnderstandingModelCandidate,
  PublicAiReasoningEffort,
  PublicAiProviderToolTargetSnapshot,
  PublicAiSettingsSnapshot,
} from "@/types";
import { ProviderToolTestDialog } from "./provider-tool-test-dialog";

// ProviderToolSelectionSettingsArea separates the system capability switch, default model, and default account.
// ProviderToolSelectionSettingsArea 分离系统能力开关、默认模型与默认账号。
export function ProviderToolSelectionSettingsArea(props: {
  // Semantic capability configured by this settings page.
  // 当前设置页面配置的语义能力。
  capability: PublicAiProviderToolCapabilityKind;
}) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [pending, setPending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // testTarget is the exact current model and account under diagnosis.
  // testTarget 是当前正在诊断的精确模型与账号。
  const [testTarget, setTestTarget] = React.useState<PublicAiProviderToolTargetSnapshot | null>(null);

  React.useEffect(() => {
    let active = true;
    void loadPublicAiSettings()
      .then((next) => {
        if (active) {
          setSnapshot(next);
          setError(null);
        }
      })
      .catch((reason) => {
        if (active) setError(errorMessage(reason));
      });
    return () => {
      active = false;
    };
  }, []);

  const targets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === props.capability
      && (!isModelExtensionCapability(props.capability)
        || target.manifestSourceLayer !== "custom"),
  ) ?? [];
  // mediaUnderstandingMode identifies the three fixed-schema understanding capabilities.
  // mediaUnderstandingMode 标识三项固定 Schema 理解能力。
  const mediaUnderstandingMode = isMediaUnderstandingCapability(props.capability);
  // mediaBackendOptions retains the inactive global route so mode changes do not reset it to the first candidate.
  // mediaBackendOptions 保留未激活的全局路由，避免切换模式时重置为首个候选。
  const mediaBackendOptions = snapshot?.mediaUnderstandingBackendOptions?.[props.capability];
  // extensionMode selects stable native multi-model configuration for generative capabilities.
  // extensionMode 为生成类能力选择稳定原生多模型配置。
  const extensionMode = isModelExtensionCapability(props.capability);
  // extensionSelections is the complete persisted target set for the current capability.
  // extensionSelections 是当前能力完整的持久化目标集合。
  const extensionSelections = snapshot?.providerExtensionTargetSets[props.capability] ?? [];
  const models = uniqueProviderToolModels(targets);
  // mediaBackendSelection is the one persisted provider-tool or model-replacement mode.
  // mediaBackendSelection 是唯一持久化的供应商工具或模型代替模式。
  const mediaBackendSelection = snapshot?.mediaUnderstandingSelections?.[props.capability];
  // defaultSelection preserves the existing direct-selection path outside media understanding.
  // defaultSelection 在媒体理解之外保留既有直连选择路径。
  const defaultSelection = mediaBackendSelection?.mode === "provider_tool"
    ? mediaBackendSelection.selection
    : snapshot?.providerToolSelections[props.capability];
  // admittedModelKeys is the shared model-list switch projected without composite-key decoding.
  // admittedModelKeys 是无需解析组合键即可投影的共享模型列表开关。
  const admittedModelKeys = new Set(
    snapshot?.multimodalUnderstandingCandidateModels?.map(
      (record) => JSON.stringify([record.providerId, record.modelId]),
    ) ?? [],
  );
  // mediaModelCandidates contains admitted exact account routes supporting this media type.
  // mediaModelCandidates 包含已准入且支持当前媒体类型的精确账号路由。
  const mediaModelCandidates = snapshot?.mediaUnderstandingModelCandidates?.filter((candidate) => (
    admittedModelKeys.has(JSON.stringify([
      candidate.selection.provider_id,
      candidate.selection.model_id,
    ]))
    && modelCandidateSupportsCapability(candidate, props.capability)
  )) ?? [];
  const selectedTarget = targets.find((target) => (
    target.connectionId === defaultSelection?.connection_id
    && target.target.target_id === defaultSelection.target_id
  )) ?? models[0] ?? null;
  const selectedModelId = selectedTarget?.target.target_id ?? "";
  const accounts = targets.filter((target) => target.target.target_id === selectedModelId);
  const selectedAccount = accounts.find((target) => (
    target.connectionId === defaultSelection?.connection_id
  )) ?? accounts[0] ?? null;
  const enabled = snapshot?.providerToolEnabledCapabilities.includes(props.capability) ?? false;

  // saveExtensionSelections persists one complete native model-extension target set.
  // saveExtensionSelections 持久化一份完整原生模型扩展目标集合。
  const saveExtensionSelections = React.useCallback(async (
    selections: Array<{ connection_id: string; target_id: string }>,
  ) => {
    setPending(true);
    try {
      const next = await savePublicAiProviderExtensionTargetSet({
        capability: props.capability,
        selections,
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }, [props.capability]);

  // saveMediaBackendSelection persists one complete closed media-understanding backend.
  // saveMediaBackendSelection 持久化一个完整封闭的媒体理解后端。
  const saveMediaBackendSelection = React.useCallback(async (
    selection: PublicAiMediaUnderstandingBackendSelection | null,
  ) => {
    setPending(true);
    try {
      // next is the authoritative snapshot after strict backend validation.
      // next 是经过严格后端校验后的权威快照。
      const next = await savePublicAiMediaUnderstandingSelection({
        capability: props.capability,
        selection,
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }, [props.capability]);

  // saveSelection persists one exact model-account pair without changing the capability switch.
  // saveSelection 持久化一个精确模型账号组合且不修改能力开关。
  const saveSelection = React.useCallback(async (target: PublicAiProviderToolTargetSnapshot) => {
    setPending(true);
    try {
      // targetSelection freezes the exact provider target and credential account.
      // targetSelection 冻结精确供应商目标与凭据账号。
      const targetSelection = {
        connection_id: target.connectionId,
        target_id: target.target.target_id,
      };
      // next is returned by the dedicated media backend RPC or the ordinary direct-target RPC.
      // next 由专用媒体后端 RPC 或普通直连目标 RPC 返回。
      const next = mediaUnderstandingMode
        ? await savePublicAiMediaUnderstandingSelection({
            capability: props.capability,
            selection: { mode: "provider_tool", selection: targetSelection },
          })
        : await savePublicAiProviderToolSelection({
            capability: props.capability,
            selection: targetSelection,
          });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }, [mediaUnderstandingMode, props.capability]);

  // saveEnabled persists only the system capability switch and initializes a missing default explicitly.
  // saveEnabled 仅持久化系统能力开关，并在缺少默认值时显式初始化。
  const saveEnabled = React.useCallback(async (nextEnabled: boolean) => {
    if (!snapshot) return;
    setPending(true);
    try {
      let next = snapshot;
      if (nextEnabled && mediaUnderstandingMode && !mediaBackendSelection) {
        if (selectedAccount) {
          next = await savePublicAiMediaUnderstandingSelection({
            capability: props.capability,
            selection: {
              mode: "provider_tool",
              selection: {
                connection_id: selectedAccount.connectionId,
                target_id: selectedAccount.target.target_id,
              },
            },
          });
        } else if (mediaModelCandidates[0]) {
          next = await savePublicAiMediaUnderstandingSelection({
            capability: props.capability,
            selection: modelReplacementSelection(mediaModelCandidates[0]),
          });
        }
      } else if (nextEnabled && !defaultSelection && selectedAccount) {
        next = await savePublicAiProviderToolSelection({
          capability: props.capability,
          selection: {
            connection_id: selectedAccount.connectionId,
            target_id: selectedAccount.target.target_id,
          },
        });
      }
      next = await savePublicAiProviderToolCapabilityEnabled({
        capability: props.capability,
        enabled: nextEnabled,
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }, [
    defaultSelection,
    mediaBackendSelection,
    mediaModelCandidates,
    mediaUnderstandingMode,
    props.capability,
    selectedAccount,
    snapshot,
  ]);

  return (
    <div className="settings-direct-content settings-capability-page">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot ? (
        <p className="settings-muted-line">{t("settings.providerTools.loading")}</p>
      ) : mediaUnderstandingMode ? (
        <MediaUnderstandingDefaults
          candidates={mediaModelCandidates}
          enabled={enabled}
          onEnabledChange={(checked) => void saveEnabled(checked)}
          onProviderTargetChange={(target) => void saveSelection(target)}
           onSelectionChange={(selection) => void saveMediaBackendSelection(selection)}
           pending={pending}
           backendOptions={mediaBackendOptions}
           selection={mediaBackendSelection}
          targets={targets}
        />
      ) : targets.length === 0 ? (
        <div className="composer-capability-dialog-empty settings-capability-empty">
          <span>{t("settings.providerTools.noTargets")}</span>
        </div>
      ) : extensionMode ? (
        <section className="settings-provider-tool-defaults">
          <header className="settings-provider-tool-defaults-header">
            <span>
              <strong>{t("settings.providerTools.systemDefault")}</strong>
              <small>选择允许 AI 查询并按需调用的原生扩展模型；工具名保持稳定。</small>
            </span>
          </header>
          <div className="settings-provider-tool-extension-list">
            {uniqueProviderToolModels(targets).map((model) => {
              // modelTargets contains every configured account for one native extension model.
              // modelTargets 包含一个原生扩展模型的全部已配置账号。
              const modelTargets = targets.filter((target) => (
                target.target.provider_id === model.target.provider_id
                && target.target.upstream_target_id === model.target.upstream_target_id
              ));
              // selectedExtension resolves the exact selected account for this model.
              // selectedExtension 解析当前模型精确选中的账号。
              const selectedExtension = extensionSelections.find((selection) => (
                modelTargets.some((target) => (
                  target.connectionId === selection.connection_id
                  && target.target.target_id === selection.target_id
                ))
              ));
              // selectedExtensionTarget is the browser target paired with the saved selection.
              // selectedExtensionTarget 是与已保存选择配对的浏览器目标。
              const selectedExtensionTarget = modelTargets.find((target) => (
                target.connectionId === selectedExtension?.connection_id
                && target.target.target_id === selectedExtension.target_id
              )) ?? modelTargets[0];
              // profile is reviewed manifest metadata displayed for this native model.
              // profile 是为当前原生模型显示的已审核清单元数据。
              const profile = model.target.contract.extension_profile;
              return (
                <article className="settings-provider-tool-extension-row" key={`${model.target.provider_id}/${model.target.upstream_target_id}`}>
                  <input
                    checked={Boolean(selectedExtension)}
                    disabled={pending || !profile}
                    onChange={(event) => {
                      const retained = extensionSelections.filter((selection) => !modelTargets.some((target) => (
                        target.connectionId === selection.connection_id
                        && target.target.target_id === selection.target_id
                      )));
                      if (event.currentTarget.checked && selectedExtensionTarget) {
                        retained.push({
                          connection_id: selectedExtensionTarget.connectionId,
                          target_id: selectedExtensionTarget.target.target_id,
                        });
                      }
                      void saveExtensionSelections(retained);
                    }}
                    type="checkbox"
                  />
                  <span className="settings-provider-tool-extension-copy">
                    <strong>{model.target.display_name}</strong>
                    <small>{profile?.summary ?? "缺少已审核模型扩展说明，当前不可选择。"}</small>
                    {profile?.characteristics?.length ? (
                      <small>{profile.characteristics.join(" · ")}</small>
                    ) : null}
                    {profile?.constraints?.length ? (
                      <small>约束：{profile.constraints.join("；")}</small>
                    ) : null}
                    {profile?.limitations?.length ? (
                      <small>限制：{profile.limitations.join("；")}</small>
                    ) : null}
                  </span>
                  <Select
                    disabled={pending || !selectedExtension}
                    onValueChange={(connectionId) => {
                      const replacement = modelTargets.find((target) => target.connectionId === connectionId);
                      if (!replacement) return;
                      const retained = extensionSelections.filter((selection) => !modelTargets.some((target) => (
                        target.connectionId === selection.connection_id
                        && target.target.target_id === selection.target_id
                      )));
                      retained.push({
                        connection_id: replacement.connectionId,
                        target_id: replacement.target.target_id,
                      });
                      void saveExtensionSelections(retained);
                    }}
                    value={selectedExtensionTarget?.connectionId ?? ""}
                  >
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent>
                      {modelTargets.map((target) => (
                        <SelectItem key={target.connectionId} value={target.connectionId}>
                          {target.connectionDisplayName}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </article>
              );
            })}
          </div>
        </section>
      ) : (
        <section className="settings-provider-tool-defaults">
          <header className="settings-provider-tool-defaults-header">
            <span>
              <strong>{t("settings.providerTools.systemDefault")}</strong>
              <small>{t("settings.providerTools.systemDefaultHint")}</small>
            </span>
            <Switch
              aria-label={enabled
                ? t("settings.providerTools.enabled")
                : t("settings.providerTools.disabled")}
              checked={enabled}
              disabled={pending}
              onCheckedChange={(checked) => void saveEnabled(checked)}
            />
          </header>
          <div className="settings-provider-tool-defaults-grid">
            <label>
              <span>{t("settings.providerTools.model")}</span>
              <Select
                disabled={pending}
                onValueChange={(targetId) => {
                  const compatible = targets.filter((target) => target.target.target_id === targetId);
                  const target = compatible.find((candidate) => (
                    candidate.connectionId === selectedAccount?.connectionId
                  )) ?? compatible[0];
                  if (target) void saveSelection(target);
                }}
                value={selectedModelId}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  {models.map((target) => (
                    <SelectItem key={target.target.target_id} value={target.target.target_id}>
                      {target.target.display_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
            <label>
              <span>{t("settings.providerTools.account")}</span>
              <Select
                disabled={pending || accounts.length === 0}
                onValueChange={(connectionId) => {
                  const target = accounts.find((candidate) => candidate.connectionId === connectionId);
                  if (target) void saveSelection(target);
                }}
                value={selectedAccount?.connectionId ?? ""}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  {accounts.map((target) => (
                    <SelectItem key={target.connectionId} value={target.connectionId}>
                      {target.connectionDisplayName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
            {props.capability === "web_search" || props.capability === "web_extract" ? (
              <Button
                aria-label={t("settings.providerTools.test")}
                disabled={pending || !selectedAccount}
                onClick={() => setTestTarget(selectedAccount)}
                size="icon-sm"
                title={t("settings.providerTools.test")}
                type="button"
                variant="outline"
              >
                <FlaskConical aria-hidden="true" size={15} />
              </Button>
            ) : null}
          </div>
        </section>
      )}
      <ProviderToolTestDialog target={testTarget} onClose={() => setTestTarget(null)} />
    </div>
  );
}

// MediaUnderstandingDefaults renders one closed mode, model, account, and reasoning selection.
// MediaUnderstandingDefaults 渲染一个封闭模式、模型、账号与思考级别选择。
function MediaUnderstandingDefaults(props: {
  // Exact admitted model-replacement account routes for the current media kind.
  // 当前媒体类型精确且已准入的模型代替账号路由。
  candidates: PublicAiMediaUnderstandingModelCandidate[];
  // Effective system capability switch.
  // 生效的系统能力开关。
  enabled: boolean;
  // Persists the independent system capability switch.
  // 持久化独立的系统能力开关。
  onEnabledChange: (enabled: boolean) => void;
  // Persists one exact provider-tool target.
  // 持久化一个精确供应商工具目标。
  onProviderTargetChange: (target: PublicAiProviderToolTargetSnapshot) => void;
  // Persists one complete closed backend selection.
  // 持久化一个完整封闭后端选择。
  onSelectionChange: (selection: PublicAiMediaUnderstandingBackendSelection) => void;
  // Disables controls while one authoritative snapshot mutation is pending.
  // 权威快照写入进行中时禁用控件。
  pending: boolean;
  // Current persisted backend selection.
  // 当前持久化后端选择。
  selection?: PublicAiMediaUnderstandingBackendSelection;
  // Independently retained global routes for the inactive backend family.
  // 未激活后端类型的独立全局保留路由。
  backendOptions?: PublicAiMediaUnderstandingBackendOptions;
  // Exact provider-tool targets available for the media kind.
  // 当前媒体类型可用的精确供应商工具目标。
  targets: PublicAiProviderToolTargetSnapshot[];
}) {
  const { t } = useI18n();
  // mode follows persistence and otherwise shows the first actually available backend family.
  // mode 遵循持久化状态；缺失时展示首个实际可用的后端类型。
  const mode = props.selection?.mode
    ?? (props.backendOptions?.provider_tool
      ? "provider_tool"
      : props.backendOptions?.model_replacement
        ? "model_replacement"
        : (props.targets.length > 0 ? "provider_tool" : "model_replacement"));
  // providerSelection exists only while the provider-specific mode is selected.
  // providerSelection 仅在选择供应商专用模式时存在。
  const providerSelection = props.selection?.mode === "provider_tool"
    ? props.selection.selection
    : props.backendOptions?.provider_tool;
  // selectedProviderTarget resolves the exact persisted account target without silent rebinding.
  // selectedProviderTarget 解析精确持久化账号目标，不静默重新绑定。
  const selectedProviderTarget = props.targets.find((target) => (
    target.connectionId === providerSelection?.connection_id
    && target.target.target_id === providerSelection?.target_id
  ));
  // providerModels collapses account-expanded provider targets into model choices.
  // providerModels 将按账号展开的供应商目标折叠为模型选择。
  const providerModels = uniqueProviderToolModels(props.targets);
  // providerModelKey preserves an unavailable persisted selection without conflating target ids across providers.
  // providerModelKey 保留不可用的持久化选择，同时避免混淆不同供应商的同名目标。
  const providerModelKey = selectedProviderTarget
    ? providerToolModelKey(selectedProviderTarget)
    : (providerSelection || !providerModels[0] ? "" : providerToolModelKey(providerModels[0]));
  // providerAccounts contains the exact credential routes for the displayed provider model.
  // providerAccounts 包含当前展示供应商模型的精确凭据路由。
  const providerAccounts = props.targets.filter(
    (target) => providerToolModelKey(target) === providerModelKey,
  );
  // modelChoices collapses exact account routes into stable provider-model choices.
  // modelChoices 将精确账号路由折叠为稳定供应商模型选择。
  const modelChoices = uniqueMediaUnderstandingModels(props.candidates);
  // persistedModelSelection exists only while model replacement is selected.
  // persistedModelSelection 仅在选择模型代替模式时存在。
  const persistedModelRoute = props.selection?.mode === "model_replacement"
    ? props.selection
    : props.backendOptions?.model_replacement;
  const persistedModelSelection = persistedModelRoute?.selection;
  // selectedModelCandidate resolves the exact persisted route without account fallback.
  // selectedModelCandidate 解析精确持久化路由，不做账号回退。
  const selectedModelCandidate = props.candidates.find((candidate) => (
    candidate.selection.provider_id === persistedModelSelection?.provider_id
    && candidate.selection.model_id === persistedModelSelection?.model_id
    && candidate.selection.connection_id === persistedModelSelection?.connection_id
  )) ?? (persistedModelSelection ? undefined : props.candidates[0]);
  // selectedModelKey identifies the shared provider-model choice independently from credentials.
  // selectedModelKey 独立于凭据标识共享供应商模型选择。
  const selectedModelKey = selectedModelCandidate
    ? mediaUnderstandingModelKey(selectedModelCandidate)
    : "";
  // modelAccounts contains exact credential routes for the displayed replacement model.
  // modelAccounts 包含当前展示替代模型的精确凭据路由。
  const modelAccounts = props.candidates.filter(
    (candidate) => mediaUnderstandingModelKey(candidate) === selectedModelKey,
  );
  // reasoningEfforts is the exact route-declared selector list.
  // reasoningEfforts 是精确路由声明的选择列表。
  const reasoningEfforts = selectedModelCandidate
    ? mediaUnderstandingReasoningEfforts(selectedModelCandidate.capabilities)
    : [];
  // selectedReasoningEffort preserves the optional persisted level.
  // selectedReasoningEffort 保留可选的持久化思考档位。
  const selectedReasoningEffort = props.selection?.mode === "model_replacement"
    ? props.selection.reasoning_effort ?? "__default__"
    : props.backendOptions?.model_replacement?.reasoning_effort ?? "__default__";
  // backendAvailable prevents enabling a capability with no concrete execution route.
  // backendAvailable 防止在没有具体执行路由时启用能力。
  const backendAvailable = props.targets.length > 0 || props.candidates.length > 0;
  return (
    <section className="settings-provider-tool-defaults">
      <header className="settings-provider-tool-defaults-header">
        <span>
          <strong>{t("settings.providerTools.systemDefault")}</strong>
          <small>{t("settings.providerTools.systemDefaultHint")}</small>
        </span>
        <Switch
          aria-label={props.enabled
            ? t("settings.providerTools.enabled")
            : t("settings.providerTools.disabled")}
          checked={props.enabled}
          disabled={mediaUnderstandingCapabilityToggleDisabled(
            props.pending,
            backendAvailable,
            props.enabled,
          )}
          onCheckedChange={props.onEnabledChange}
        />
      </header>
      {!backendAvailable ? (
        <div className="composer-capability-dialog-empty settings-capability-empty">
          <span>{t("settings.providerTools.noTargets")}</span>
        </div>
      ) : (
        <div className="settings-provider-tool-defaults-grid">
          <label>
            <span>{t("settings.providerTools.mediaBackendMode")}</span>
            <Select
              disabled={props.pending}
              onValueChange={(nextMode) => {
                if (nextMode === "provider_tool") {
                  const retainedTarget = props.targets.find((target) => (
                    target.connectionId === props.backendOptions?.provider_tool?.connection_id
                    && target.target.target_id === props.backendOptions?.provider_tool?.target_id
                  )) ?? props.targets[0];
                  if (retainedTarget) props.onProviderTargetChange(retainedTarget);
                } else if (nextMode === "model_replacement") {
                  const retainedCandidate = props.candidates.find((candidate) => (
                    candidate.selection.provider_id === props.backendOptions?.model_replacement?.selection.provider_id
                    && candidate.selection.model_id === props.backendOptions?.model_replacement?.selection.model_id
                    && candidate.selection.connection_id === props.backendOptions?.model_replacement?.selection.connection_id
                  ));
                  const candidate = retainedCandidate ?? props.candidates[0];
                  if (candidate) {
                    props.onSelectionChange(modelReplacementSelection(
                      candidate,
                      retainedCandidate
                        ? props.backendOptions?.model_replacement?.reasoning_effort
                        : undefined,
                    ));
                  }
                }
              }}
              value={mode}
            >
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                {props.targets.length > 0 ? (
                  <SelectItem value="provider_tool">
                    {t("settings.providerTools.providerToolMode")}
                  </SelectItem>
                ) : null}
                {props.candidates.length > 0 ? (
                  <SelectItem value="model_replacement">
                    {t("settings.providerTools.modelReplacementMode")}
                  </SelectItem>
                ) : null}
              </SelectContent>
            </Select>
          </label>
          {mode === "provider_tool" ? (
            <>
              <label>
                <span>{t("settings.providerTools.model")}</span>
                <Select
                  disabled={props.pending}
                  onValueChange={(modelKey) => {
                    // compatibleTarget retains the selected account when the new model exposes it.
                    // compatibleTarget 在新模型公开已选账号时保留该账号。
                    const compatibleTarget = props.targets.find((target) => (
                      providerToolModelKey(target) === modelKey
                      && target.connectionId === selectedProviderTarget?.connectionId
                    )) ?? props.targets.find((target) => providerToolModelKey(target) === modelKey);
                    if (compatibleTarget) props.onProviderTargetChange(compatibleTarget);
                  }}
                  value={providerModelKey}
                >
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {providerModels.map((target) => (
                      <SelectItem key={providerToolModelKey(target)} value={providerToolModelKey(target)}>
                        {target.target.display_name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label>
                <span>{t("settings.providerTools.account")}</span>
                <Select
                  disabled={props.pending || providerAccounts.length === 0}
                  onValueChange={(connectionId) => {
                    // replacementTarget is the exact selected provider credential route.
                    // replacementTarget 是精确选中的供应商凭据路由。
                    const replacementTarget = providerAccounts.find(
                      (target) => target.connectionId === connectionId,
                    );
                    if (replacementTarget) props.onProviderTargetChange(replacementTarget);
                  }}
                  value={selectedProviderTarget?.connectionId ?? ""}
                >
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {providerAccounts.map((target) => (
                      <SelectItem key={target.connectionId} value={target.connectionId}>
                        {target.connectionDisplayName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            </>
          ) : (
            <>
              <label>
                <span>{t("settings.providerTools.model")}</span>
                <Select
                  disabled={props.pending}
                  onValueChange={(modelKey) => {
                    // replacementCandidate is the first exact account route for the explicit model choice.
                    // replacementCandidate 是显式模型选择的首条精确账号路由。
                    const replacementCandidate = props.candidates.find(
                      (candidate) => mediaUnderstandingModelKey(candidate) === modelKey,
                    );
                    if (replacementCandidate) {
                      props.onSelectionChange(modelReplacementSelection(replacementCandidate));
                    }
                  }}
                  value={selectedModelKey}
                >
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {modelChoices.map((candidate) => (
                      <SelectItem
                        key={mediaUnderstandingModelKey(candidate)}
                        value={mediaUnderstandingModelKey(candidate)}
                      >
                        {candidate.providerDisplayName} · {candidate.modelDisplayName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label>
                <span>{t("settings.providerTools.account")}</span>
                <Select
                  disabled={props.pending || modelAccounts.length === 0}
                  onValueChange={(connectionId) => {
                    // replacementCandidate is the exact selected model credential route.
                    // replacementCandidate 是精确选中的模型凭据路由。
                    const replacementCandidate = modelAccounts.find(
                      (candidate) => candidate.selection.connection_id === connectionId,
                    );
                    if (replacementCandidate) {
                      props.onSelectionChange(modelReplacementSelection(
                        replacementCandidate,
                        props.selection?.mode === "model_replacement"
                          ? props.selection.reasoning_effort
                          : undefined,
                      ));
                    }
                  }}
                  value={selectedModelCandidate?.selection.connection_id ?? ""}
                >
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {modelAccounts.map((candidate) => (
                      <SelectItem
                        key={candidate.selection.connection_id}
                        value={candidate.selection.connection_id}
                      >
                        {candidate.connectionDisplayName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label>
                <span>{t("settings.providerTools.reasoningEffort")}</span>
                <Select
                  disabled={props.pending || !selectedModelCandidate}
                  onValueChange={(effort) => {
                    if (!selectedModelCandidate) return;
                    props.onSelectionChange(modelReplacementSelection(
                      selectedModelCandidate,
                      effort === "__default__"
                        ? undefined
                        : effort as PublicAiReasoningEffort,
                    ));
                  }}
                  value={selectedReasoningEffort}
                >
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__default__">
                      {t("settings.providerTools.modelDefaultReasoning")}
                    </SelectItem>
                    {reasoningEfforts.map((effort) => (
                      <SelectItem key={effort} value={effort}>{effort}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            </>
          )}
        </div>
      )}
    </section>
  );
}

// mediaUnderstandingCapabilityToggleDisabled blocks invalid enablement while preserving stale-state cleanup.
// mediaUnderstandingCapabilityToggleDisabled 阻止无效启用，同时保留清理失效旧状态的能力。
//
// Parameters: pending reports an active mutation, backendAvailable reports a concrete route, and enabled is persisted state.
// 参数：pending 表示写入进行中，backendAvailable 表示存在具体路由，enabled 是持久化状态。
//
// Returns: true only when the switch must reject every state transition.
// 返回值：仅当开关必须拒绝全部状态转换时返回 true。
export function mediaUnderstandingCapabilityToggleDisabled(
  pending: boolean,
  backendAvailable: boolean,
  enabled: boolean,
): boolean {
  return pending || (!backendAvailable && !enabled);
}

// mediaUnderstandingReasoningEfforts returns every explicit level accepted by one exact route.
// mediaUnderstandingReasoningEfforts 返回一条精确路由接受的全部显式思考档位。
function mediaUnderstandingReasoningEfforts(
  capabilities: PublicAiMediaUnderstandingModelCandidate["capabilities"],
): string[] {
  // efforts is the provider-declared selectable list before fixed-control normalization.
  // efforts 是固定控制规范化前供应商声明的可选列表。
  const efforts = capabilities.reasoning_efforts ?? [];
  switch (capabilities.reasoning_control ?? "unknown") {
    case "fixed_off":
      return ["none"];
    case "fixed_on":
      return efforts.filter((effort) => effort !== "none");
    case "toggle":
    case "effort_selection":
    case "provider_adaptive":
      return efforts;
    default:
      return [];
  }
}

// isMediaUnderstandingCapability reports whether one capability uses the fixed media-understanding contract.
// isMediaUnderstandingCapability 报告一项能力是否使用固定媒体理解契约。
function isMediaUnderstandingCapability(
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  return capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand";
}

// modelCandidateSupportsCapability applies the exact input resource and structured-output gate.
// modelCandidateSupportsCapability 应用精确输入资源与结构化输出门槛。
function modelCandidateSupportsCapability(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  // requiredResource is the fixed non-text input required by the selected semantic capability.
  // requiredResource 是所选语义能力要求的固定非文本输入。
  const requiredResource = capability === "image_understand"
    ? "image"
    : capability === "video_understand"
      ? "video"
      : capability === "audio_understand"
        ? "audio"
        : null;
  return requiredResource !== null
    && candidate.capabilities.structured_output !== "unsupported"
    && candidate.capabilities.operations?.includes("conversation_respond") === true
    && candidate.capabilities.input_resources?.includes(requiredResource) === true;
}

// mediaUnderstandingModelKey returns one browser-local provider-model identity.
// mediaUnderstandingModelKey 返回一个浏览器本地供应商模型身份。
function mediaUnderstandingModelKey(candidate: PublicAiMediaUnderstandingModelCandidate): string {
  return JSON.stringify([
    candidate.selection.provider_id,
    candidate.selection.model_id,
  ]);
}

// uniqueMediaUnderstandingModels collapses exact account routes into stable model choices.
// uniqueMediaUnderstandingModels 将精确账号路由折叠为稳定模型选择。
function uniqueMediaUnderstandingModels(
  candidates: PublicAiMediaUnderstandingModelCandidate[],
): PublicAiMediaUnderstandingModelCandidate[] {
  // models retains the first backend-sorted account route for each provider-model identity.
  // models 为每个供应商模型身份保留后端排序的首条账号路由。
  const models = new Map<string, PublicAiMediaUnderstandingModelCandidate>();
  for (const candidate of candidates) {
    // modelKey is collision-free across providers that reuse upstream model identifiers.
    // modelKey 在复用上游模型标识的供应商之间避免冲突。
    const modelKey = mediaUnderstandingModelKey(candidate);
    if (!models.has(modelKey)) models.set(modelKey, candidate);
  }
  return Array.from(models.values());
}

// modelReplacementSelection freezes one candidate and optional reasoning level into the closed backend union.
// modelReplacementSelection 将一个候选与可选思考档位冻结为封闭后端联合类型。
function modelReplacementSelection(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiMediaUnderstandingBackendSelection {
  return {
    mode: "model_replacement",
    selection: candidate.selection,
    ...(reasoningEffort ? { reasoning_effort: reasoningEffort } : {}),
  };
}

// uniqueProviderToolModels collapses account-expanded targets into stable model rows.
// uniqueProviderToolModels 将按账号展开的目标折叠为稳定模型行。
export function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    // modelKey prevents collisions between providers that reuse an upstream model id.
    // modelKey 防止复用上游模型标识的不同供应商发生冲突。
    const modelKey = providerToolModelKey(target);
    if (!models.has(modelKey)) models.set(modelKey, target);
  }
  return Array.from(models.values()).sort((left, right) => (
    left.target.display_name.localeCompare(right.target.display_name)
    || left.target.target_id.localeCompare(right.target.target_id)
  ));
}

// providerToolModelKey returns one collision-free browser identity for a provider-native model.
// providerToolModelKey 返回供应商原生模型在浏览器中的无碰撞身份。
export function providerToolModelKey(target: PublicAiProviderToolTargetSnapshot): string {
  return JSON.stringify([
    target.target.provider_id,
    target.target.upstream_target_id,
  ]);
}

// errorMessage converts a rejected value into a visible diagnostic.
// errorMessage 将被拒绝的值转换为可见诊断。
function errorMessage(reason: unknown): string {
  // message preserves an actionable explanation when the desktop halves are from different builds.
  // message 在桌面前后端来自不同构建时保留可执行的说明。
  const message = reason instanceof Error ? reason.message : String(reason);
  if (message.includes("unknown service RPC method")) {
    return "当前前端与桌面后端版本不一致，请彻底退出应用后重新编译并启动。";
  }
  return message;
}
