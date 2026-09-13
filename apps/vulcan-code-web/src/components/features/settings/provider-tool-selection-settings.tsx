import * as React from "react";
import { FlaskConical } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { ProviderToolExtensionModelCard } from "@/components/shared/provider-tool-extension-model-card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import {
  isModelExtensionCapability,
  mediaUnderstandingCapabilityToggleDisabled,
  modelCapabilitiesSupportMediaUnderstanding,
} from "@/lib/provider-tool-capabilities";
import {
  mediaReplacementReasoningEfforts,
  mediaReplacementReasoningOverrides,
  PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS,
} from "@/lib/reasoning-effort-labels";
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
// Parameters: props identifies the capability settings scope; returns stable default-target controls and explicit loading/error recovery.
// 参数：props 标识能力设置范围；返回稳定默认目标控件及明确的加载和错误恢复界面。
export function ProviderToolSelectionSettingsArea(props: {
  // Semantic capability configured by this settings page.
  // 当前设置页面配置的语义能力。
  capability: PublicAiProviderToolCapabilityKind;
}) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [pending, setPending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // loadAttempt retries failed initial reads without replaying a settings write.
  // loadAttempt 重试失败的首次读取，不重放设置写入。
  const [loadAttempt, setLoadAttempt] = React.useState(0);
  // testTarget is the exact current model and account under diagnosis.
  // testTarget 是当前正在诊断的精确模型与账号。
  const [testTarget, setTestTarget] = React.useState<PublicAiProviderToolTargetSnapshot | null>(null);

  React.useEffect(() => {
    let active = true;
    setError(null);
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
  }, [loadAttempt]);

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
    && modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, props.capability)
  )) ?? [];
  const selectedTarget = targets.find((target) => (
    target.connectionId === defaultSelection?.connection_id
    && target.target.target_id === defaultSelection.target_id
  )) ?? null;
  const selectedModelId = selectedTarget?.target.target_id ?? "";
  const accounts = targets.filter((target) => target.target.target_id === selectedModelId);
  const selectedAccount = accounts.find((target) => (
    target.connectionId === defaultSelection?.connection_id
  )) ?? null;
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
      return true;
    } catch (reason) {
      setError(errorMessage(reason));
      return false;
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
      return true;
    } catch (reason) {
      setError(errorMessage(reason));
      return false;
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
      return true;
    } catch (reason) {
      setError(errorMessage(reason));
      return false;
    } finally {
      setPending(false);
    }
  }, [mediaUnderstandingMode, props.capability]);

  // saveEnabled preserves the selected understanding mode; ordinary tools may initialize their direct target.
  // saveEnabled 保留已选理解模式；普通工具可以初始化其直连目标。
  const saveEnabled = React.useCallback(async (nextEnabled: boolean) => {
    if (!snapshot || (nextEnabled && !mediaUnderstandingMode && !selectedAccount)) return false;
    setPending(true);
    try {
      let next = snapshot;
      if (nextEnabled && !mediaUnderstandingMode && !defaultSelection && selectedAccount) {
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
      return true;
    } catch (reason) {
      setError(errorMessage(reason));
      return false;
    } finally {
      setPending(false);
    }
  }, [
    defaultSelection,
    mediaUnderstandingMode,
    props.capability,
    selectedAccount,
    snapshot,
  ]);

  return (
    <div className="settings-direct-content settings-capability-page">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot && error ? <Button onClick={() => setLoadAttempt((value) => value + 1)} type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button> : !snapshot ? (
        <p className="settings-muted-line">{t("settings.providerTools.loading")}</p>
      ) : mediaUnderstandingMode ? (
        <MediaUnderstandingDefaults
          candidates={mediaModelCandidates}
          enabled={enabled}
          key={props.capability}
          onEnabledChange={saveEnabled}
          onProviderTargetChange={saveSelection}
           onSelectionChange={saveMediaBackendSelection}
           pending={pending}
           backendOptions={mediaBackendOptions}
           selection={mediaBackendSelection}
          targets={targets}
        />
      ) : extensionMode ? (
        <section className="settings-provider-tool-defaults">
          <header className="settings-provider-tool-defaults-header">
            <span>
              <strong>{t("settings.providerTools.systemDefault")}</strong>
              <small>{t("settings.modelExperience.extensionSelectionHint")}</small>
            </span>
          </header>
          {targets.length === 0 ? (
            <>
              <p className="settings-muted-line">{t("settings.providerTools.noTargets")}</p>
              <div className="settings-provider-tool-defaults-grid">
                <label>
                  <span>{t("settings.providerTools.model")}</span>
                  <Select disabled value="">
                    <SelectTrigger aria-label={t("settings.providerTools.model")}><SelectValue placeholder={t("settings.providerTools.model")} /></SelectTrigger>
                    <SelectContent />
                  </Select>
                </label>
                <label>
                  <span>{t("settings.providerTools.account")}</span>
                  <Select disabled value="">
                    <SelectTrigger aria-label={t("settings.providerTools.account")}><SelectValue placeholder={t("settings.providerTools.account")} /></SelectTrigger>
                    <SelectContent />
                  </Select>
                </label>
              </div>
            </>
          ) : null}
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
                <ProviderToolExtensionModelCard
                  accountControl={(
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
                  )}
                  characteristics={profile?.characteristics?.length
                    ? profile.characteristics.join(" · ")
                    : null}
                  checked={Boolean(selectedExtension)}
                  checkboxDisabled={pending || !profile}
                  constraints={t("composer.capability.extensionConstraints").replace(
                    "{value}",
                    profile?.constraints?.length
                      ? profile.constraints.join(t("global.listDelimiter"))
                      : "—",
                  )}
                  key={`${model.target.provider_id}/${model.target.upstream_target_id}`}
                  limitations={t("composer.capability.extensionLimitations").replace(
                    "{value}",
                    profile?.limitations?.length
                      ? profile.limitations.join(t("global.listDelimiter"))
                      : "—",
                  )}
                  modelId={model.target.upstream_target_id}
                  onCheckedChange={(checked) => {
                    const retained = extensionSelections.filter((selection) => !modelTargets.some((target) => (
                      target.connectionId === selection.connection_id
                      && target.target.target_id === selection.target_id
                    )));
                    if (checked && selectedExtensionTarget) {
                      retained.push({
                        connection_id: selectedExtensionTarget.connectionId,
                        target_id: selectedExtensionTarget.target.target_id,
                      });
                    }
                    void saveExtensionSelections(retained);
                  }}
                  providerId={model.target.provider_id}
                  summary={profile?.summary ?? t("composer.capability.extensionProfileMissing")}
                  title={model.target.display_name}
                />
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
              aria-label={enabled && Boolean(selectedAccount)
                ? t("settings.providerTools.enabled")
                : t("settings.providerTools.disabled")}
              checked={enabled && Boolean(selectedAccount)}
              disabled={pending || !selectedAccount}
              onCheckedChange={(checked) => void saveEnabled(checked)}
            />
          </header>
          {targets.length === 0 ? <p className="settings-muted-line">{t("settings.providerTools.noTargets")}</p> : null}
          <div className="settings-provider-tool-defaults-grid">
            <label>
              <span>{t("settings.providerTools.model")}</span>
              <Select
                disabled={pending || models.length === 0}
                onValueChange={(targetId) => {
                  const compatible = targets.filter((target) => target.target.target_id === targetId);
                  const target = compatible.find((candidate) => (
                    candidate.connectionId === selectedAccount?.connectionId
                  )) ?? compatible[0];
                  if (target) void saveSelection(target);
                }}
                value={selectedModelId}
              >
                <SelectTrigger aria-label={t("settings.providerTools.model")}><SelectValue placeholder={t("settings.providerTools.model")} /></SelectTrigger>
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
                <SelectTrigger aria-label={t("settings.providerTools.account")}><SelectValue placeholder={t("settings.providerTools.account")} /></SelectTrigger>
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
// Parameters: props supplies the exact understanding capability, stored routes and save callbacks; returns both modes and only enables an available selected target.
// 参数：props 提供精确理解能力、已存路由及保存回调；返回两种模式，并仅允许启用可用的已选目标。
export function MediaUnderstandingDefaults(props: {
  // Exact admitted model-replacement account routes for the current media kind.
  // 当前媒体类型精确且已准入的模型代替账号路由。
  candidates: PublicAiMediaUnderstandingModelCandidate[];
  // Effective system capability switch.
  // 生效的系统能力开关。
  enabled: boolean;
  // Persists the independent system capability switch.
  // 持久化独立的系统能力开关。
  onEnabledChange: (enabled: boolean) => void | Promise<boolean | undefined>;
  // Persists one exact provider-tool target.
  // 持久化一个精确供应商工具目标。
  onProviderTargetChange: (target: PublicAiProviderToolTargetSnapshot) => void | Promise<boolean>;
  // Persists one complete closed backend selection.
  // 持久化一个完整封闭后端选择。
  onSelectionChange: (selection: PublicAiMediaUnderstandingBackendSelection) => void | Promise<boolean>;
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
  // mode retains an explicit empty editing mode without fabricating a persisted target.
  // mode 保留显式选择的空编辑模式，不伪造持久化目标。
  const [mode, setMode] = React.useState<PublicAiMediaUnderstandingBackendSelection["mode"]>(
    props.selection?.mode ?? "provider_tool",
  );
  // switching prevents overlapping mode transitions before the parent receives its updated snapshot.
  // switching 在父组件收到更新快照前防止模式切换重叠。
  const [switching, setSwitching] = React.useState(false);
  // transitionInFlight closes the synchronous double-click gap before React commits disabled controls.
  // transitionInFlight 关闭 React 提交禁用控件前的同步双击间隙。
  const transitionInFlight = React.useRef(false);
  // Follow successful route changes while retaining empty mode edits across switch snapshots.
  // 跟随成功保存的路由变更，同时在开关快照刷新时保留空模式编辑。
  React.useEffect(() => {
    if (props.selection) setMode(props.selection.mode);
  }, [props.selection?.mode]);
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
    : "";
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
  ));
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
    ? mediaReplacementReasoningOverrides(selectedModelCandidate.capabilities)
    : [];
  // selectedReasoningEffort preserves the optional persisted level.
  // selectedReasoningEffort 保留可选的持久化思考档位。
  const selectedReasoningEffort = props.selection?.mode === "model_replacement"
    ? props.selection.reasoning_effort ?? "__default__"
    : props.backendOptions?.model_replacement?.reasoning_effort ?? "__default__";
  // backendAvailable prevents enabling a capability with no concrete execution route.
  // backendAvailable 防止在没有具体执行路由时启用能力。
  const backendAvailable = props.selection?.mode === mode && (mode === "provider_tool"
    ? Boolean(selectedProviderTarget)
    : Boolean(selectedModelCandidate));
  // Effective presentation requires the exact selected executable target; a retained enabled bit alone is insufficient.
  // 有效显示状态要求精确选中的可执行目标，单独保留的启用位不足以生效。
  const presentationEnabled = props.enabled && backendAvailable;
  // changeMode saves a valid retained route or disables an empty mode while preserving both stored routes.
  // changeMode 保存有效保留路由，或关闭空模式默认，同时保留两条已存路由。
  // The exact destination mode is committed only after its required save succeeds.
  // 仅在必需保存成功后才提交精确的目标模式。
  // Parameters: nextMode is the explicitly requested closed mode; returns a promise after its required save, retaining the old visible mode on failure.
  // 参数：nextMode 是明确请求的封闭模式；返回所需保存结束后的 Promise，失败时保留旧可见模式。
  const changeMode = async (nextMode: PublicAiMediaUnderstandingBackendSelection["mode"]) => {
    if (transitionInFlight.current || props.pending || nextMode === mode) return;
    transitionInFlight.current = true;
    setSwitching(true);
    try {
      if (nextMode === "provider_tool") {
        // target restores the retained exact route, then allows an explicit mode switch to initialize its first choice.
        // target 恢复保留的精确路由，再允许显式模式切换初始化首个选项。
        const target = selectedProviderTarget ?? props.targets[0];
        if (target) {
          if (await props.onProviderTargetChange(target) === false) return;
        } else if (props.enabled && await props.onEnabledChange(false) === false) return;
      } else {
        // candidate preserves the retained route and only keeps reasoning supported by that exact route.
        // candidate 保留已存路由，并只保留该精确路由支持的思考档位。
        const candidate = selectedModelCandidate ?? props.candidates[0];
        if (candidate) {
          if (await props.onSelectionChange(modelReplacementSelection(candidate,
            selectedModelCandidate ? supportedMediaReplacementReasoningEffort(candidate, persistedModelRoute?.reasoning_effort) : undefined,
          )) === false) return;
        } else if (props.enabled && await props.onEnabledChange(false) === false) return;
      }
      setMode(nextMode);
    } finally {
      transitionInFlight.current = false;
      setSwitching(false);
    }
  };
  return (
    <section className="settings-provider-tool-defaults">
      <header className="settings-provider-tool-defaults-header">
        <span>
          <strong>{t("settings.providerTools.systemDefault")}</strong>
          <small>{t("settings.providerTools.systemDefaultHint")}</small>
        </span>
        <Switch
          aria-label={presentationEnabled
            ? t("settings.providerTools.enabled")
            : t("settings.providerTools.disabled")}
          checked={presentationEnabled}
          disabled={mediaUnderstandingCapabilityToggleDisabled(
            props.pending || switching,
            backendAvailable,
            presentationEnabled,
          )}
          onCheckedChange={props.onEnabledChange}
        />
      </header>
        <div className="settings-provider-tool-defaults-grid settings-media-understanding-grid">
          <div className="settings-media-mode-field">
            <span>{t("settings.providerTools.mediaBackendMode")}</span>
            <div className="settings-media-mode-options" role="group" aria-label={t("settings.providerTools.mediaBackendMode")}>
              <Button aria-pressed={mode === "provider_tool"} disabled={props.pending || switching} onClick={() => void changeMode("provider_tool")} type="button" variant={mode === "provider_tool" ? "secondary" : "outline"}>
                {t("settings.providerTools.providerToolMode")}
              </Button>
              <Button aria-pressed={mode === "model_replacement"} disabled={props.pending || switching} onClick={() => void changeMode("model_replacement")} type="button" variant={mode === "model_replacement" ? "secondary" : "outline"}>
                {t("settings.providerTools.modelReplacementMode")}
              </Button>
            </div>
          </div>
          {mode === "provider_tool" ? (
            <>
              <label>
                <span>{t("settings.providerTools.model")}</span>
                <Combobox
                  ariaLabel={t("settings.providerTools.model")}
                  emptyText={t("settings.modelExperience.modelNoMatch")}
                  searchPlaceholder={t("settings.modelExperience.modelSearch")}
                  placeholder={t("settings.providerTools.selectModel")}
                  disabled={props.pending || providerModels.length === 0}
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
                  options={providerModels.map((target) => ({
                    value: providerToolModelKey(target), label: target.target.display_name,
                    description: target.target.provider_id,
                    keywords: [target.target.provider_id, target.target.upstream_target_id,
                      ...props.targets.filter((candidate) => providerToolModelKey(candidate) === providerToolModelKey(target)).flatMap((candidate) => [candidate.connectionId, candidate.connectionDisplayName])],
                  }))}
                />
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
                  <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectAccount")} /></SelectTrigger>
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
                <Combobox
                  ariaLabel={t("settings.providerTools.model")}
                  emptyText={t("settings.modelExperience.modelNoMatch")}
                  searchPlaceholder={t("settings.modelExperience.modelSearch")}
                  placeholder={t("settings.providerTools.selectModel")}
                  disabled={props.pending || modelChoices.length === 0}
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
                  options={modelChoices.map((candidate) => ({
                    value: mediaUnderstandingModelKey(candidate), label: candidate.modelDisplayName,
                    description: candidate.providerDisplayName,
                    keywords: [candidate.selection.provider_id, candidate.selection.model_id,
                      ...props.candidates.filter((route) => mediaUnderstandingModelKey(route) === mediaUnderstandingModelKey(candidate)).flatMap((route) => [route.connectionDisplayName, route.selection.connection_id ?? ""])],
                  }))}
                />
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
                          ? supportedMediaReplacementReasoningEffort(
                            replacementCandidate,
                            props.selection.reasoning_effort,
                          )
                          : undefined,
                      ));
                    }
                  }}
                  value={selectedModelCandidate?.selection.connection_id ?? ""}
                >
                  <SelectTrigger>
                    <SelectValue placeholder={t("settings.providerTools.selectAccount")} />
                  </SelectTrigger>
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
              {reasoningEfforts.length > 0 ? (
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
                            <SelectItem key={effort} value={effort}>
                              {t(PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS[effort])}
                            </SelectItem>
                          ))}
                    </SelectContent>
                  </Select>
                </label>
              ) : null}
            </>
          )}
        </div>
      {!backendAvailable ? (
        <p className="settings-media-understanding-status" role="status">
          {(mode === "provider_tool" ? props.targets.length : props.candidates.length) === 0
            ? t("settings.providerTools.noTargets")
            : t("settings.providerTools.noSelection")}
        </p>
      ) : null}
    </section>
  );
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

// supportedMediaReplacementReasoningEffort retains an effort only when the exact route permits user override.
// supportedMediaReplacementReasoningEffort 仅在精确路由允许用户覆写时保留思考档位。
function supportedMediaReplacementReasoningEffort(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiReasoningEffort | undefined {
  if (!reasoningEffort) return undefined;
  return mediaReplacementReasoningEfforts(candidate.capabilities).includes(reasoningEffort)
    ? reasoningEffort
    : undefined;
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
