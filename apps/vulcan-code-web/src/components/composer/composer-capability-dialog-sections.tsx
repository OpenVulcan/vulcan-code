import React from "react";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { AlertTriangle } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { SegmentedControl, SegmentedControlItem } from "@/components/ui/choice-controls";
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { ProviderToolExtensionModelCard } from "@/components/shared/provider-tool-extension-model-card";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import { mediaUnderstandingCapabilityToggleDisabled } from "@/lib/provider-tool-capabilities";
import { PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS } from "@/lib/reasoning-effort-labels";
import type {
  PublicAiMediaUnderstandingBackendMode,
  PublicAiProviderToolCapabilityKind,
  PublicAiReasoningEffort,
} from "@/types";
import {
  type PublicAiMediaUnderstandingCapabilityKind,
  isMediaUnderstandingCapability,
  mediaUnderstandingModelKey,
  modelReplacementSelection,
  providerMediaUnderstandingSelection,
  providerToolCapabilityColor,
  providerToolCapabilityLabel,
  providerToolCapabilitySwitchDisabled,
  providerToolModelKey,
  selectionForTarget,
  supportedMediaReplacementReasoningEffort,
} from "@/components/composer/composer-capability-model";
import type { ComposerCapabilitySettings } from "@/components/composer/use-composer-capability-settings";

// ComposerCapabilityKindList renders one tab's capability rows with their switches and status lines.
// ComposerCapabilityKindList 渲染一个标签内的能力行及其开关与状态行。
export function ComposerCapabilityKindList(props: {
  // Session capability state and actions from useComposerCapabilitySettings.
  // 来自 useComposerCapabilitySettings 的会话能力状态与操作。
  settings: ComposerCapabilitySettings;
  // Active intent tab whose capabilities are listed.
  // 列出其能力的当前意图标签。
  tab: { label: string; capabilities: PublicAiProviderToolCapabilityKind[] };
  // Receives the selected row button so recovery and retry can restore focus to it.
  // 接收选中行按钮，供恢复与重试操作将焦点还原到该按钮。
  selectedCategoryButton: React.RefObject<HTMLButtonElement | null>;
}) {
  const { t } = useI18n();
  const activeCapabilityTab = props.tab;
  const selectedCategoryButton = props.selectedCategoryButton;
  const {
    selectedCapability,
    setSelectedCapability,
    setDraft,
    saving,
    describeCapability,
  } = props.settings;
  return (
    <aside aria-label={activeCapabilityTab.label} className="composer-capability-dialog-left">
      {activeCapabilityTab.capabilities.map((capability) => {
        const { directMode, extensionSelectionCount, mediaAvailable, enabled, targets, targetCount, conflict, subtitle } =
          describeCapability(capability);
        return (
          <div
            className={cn(
              "composer-capability-kind-row",
              enabled && "enabled",
              selectedCapability === capability && "selected",
              conflict && "conflict",
            )}
            aria-invalid={conflict ? true : undefined}
            key={capability}
          >
            {directMode ? (
              <input
                aria-label={`${providerToolCapabilityLabel(capability, t)} · ${t("settings.providerTools.enabled")}`}
                checked={enabled}
                disabled={isMediaUnderstandingCapability(capability)
                  ? mediaUnderstandingCapabilityToggleDisabled(saving, mediaAvailable, enabled)
                  : providerToolCapabilitySwitchDisabled(saving, targetCount, enabled)}
                onChange={(event) => {
                  event.stopPropagation();
                  const checked = event.currentTarget.checked;
                  setDraft((current) => {
                    const enabledCapabilities = new Set(current.enabledCapabilities);
                    if (!checked) {
                      enabledCapabilities.delete(capability);
                      return { ...current, enabledCapabilities: Array.from(enabledCapabilities).sort() };
                    }
                    enabledCapabilities.add(capability);
                    if (isMediaUnderstandingCapability(capability)) {
                      if (!mediaAvailable) return current;
                      return {
                        ...current,
                        enabledCapabilities: Array.from(enabledCapabilities).sort(),
                      };
                    }
                    const first = targets[0];
                    if (first) {
                      return {
                        ...current,
                        enabledCapabilities: Array.from(enabledCapabilities).sort(),
                        selections: current.selections[capability] ? current.selections : {
                          ...current.selections,
                          [capability]: selectionForTarget(first),
                        },
                      };
                    }
                    return { ...current, enabledCapabilities: Array.from(enabledCapabilities).sort() };
                  });
                }}
                onClick={(event) => event.stopPropagation()}
                type="checkbox"
              />
            ) : (
              <span aria-hidden="true" className={cn("composer-capability-extension-count", extensionSelectionCount > 0 && "active")}>
                {extensionSelectionCount > 0 ? extensionSelectionCount : null}
              </span>
            )}
            <Button className="composer-capability-kind-select h-auto min-w-0 flex-1 justify-start whitespace-normal text-left" type="button" variant="ghost"
              ref={selectedCapability === capability ? selectedCategoryButton : undefined}
              aria-pressed={selectedCapability === capability} disabled={saving}
              onClick={() => setSelectedCapability(capability)}>
              <span className={cn(
                "composer-capability-colored-icon",
                enabled ? "enabled" : "disabled",
              )} style={{
                "--capability-color": providerToolCapabilityColor(capability),
              } as React.CSSProperties}>
                <ProviderToolCapabilityIcon capability={capability} size={16} />
              </span>
              <span className="composer-capability-kind-copy">
                <span className="composer-capability-kind-title">
                  {providerToolCapabilityLabel(capability, t)}
                  {conflict ? <AlertTriangle aria-hidden="true" size={13} /> : null}
                </span>
                <small>{subtitle}</small>
                {conflict ? (
                  <small className="composer-capability-node-warning" role="alert">
                    {t(conflict.kind === "missing_selection"
                      ? "composer.capability.conflict.missingSelection"
                      : "composer.capability.conflict.targetUnavailable")}
                  </small>
                ) : null}
              </span>
            </Button>
          </div>
        );
      })}
    </aside>
  );
}

// ComposerCapabilityMediaRoute edits one media-understanding route across provider-tool and model-replacement modes.
// ComposerCapabilityMediaRoute 在供应商工具与模型代替两种模式间编辑一条媒体理解路由。
export function ComposerCapabilityMediaRoute(props: {
  // Session capability state and actions from useComposerCapabilitySettings.
  // 来自 useComposerCapabilitySettings 的会话能力状态与操作。
  settings: ComposerCapabilitySettings;
  // Selected media facade being edited.
  // 正在编辑的媒体外壳能力。
  capability: PublicAiMediaUnderstandingCapabilityKind;
}) {
  const { t } = useI18n();
  const selectedCapability = props.capability;
  const {
    setDraft,
    saving,
    mutationPending,
    setMediaEditModes,
    updateMediaSelection,
    selectedMediaTargets,
    selectedMediaProviderModels,
    selectedMediaCandidates,
    selectedMediaModelChoices,
    selectedMediaSelection,
    selectedMediaMode,
    selectedMediaProviderTarget,
    selectedMediaProviderModelKey,
    selectedMediaProviderAccounts,
    selectedMediaModelCandidate,
    selectedMediaModelKey,
    selectedMediaModelAccounts,
    selectedMediaReasoningEfforts,
  } = props.settings;
  return (
    <div className="composer-capability-route-selectors">
      <label>
        <span>{t("composer.capability.mediaBackendMode")}</span>
        <SegmentedControl
          aria-label={t("composer.capability.mediaBackendMode")}
          disabled={saving}
          value={selectedMediaMode}
          onValueChange={(value) => {
            const mode = value as PublicAiMediaUnderstandingBackendMode;
            if (mutationPending.current) return;
            setMediaEditModes((current) => ({ ...current, [selectedCapability]: mode }));
            const selection = mode === "provider_tool"
              ? selectedMediaTargets[0]
                ? providerMediaUnderstandingSelection(selectedMediaTargets[0])
                : undefined
              : selectedMediaCandidates[0]
                ? modelReplacementSelection(selectedMediaCandidates[0])
                : undefined;
            if (selection) {
              updateMediaSelection(selectedCapability, selection);
            } else {
              setDraft((current) => ({ ...current,
                enabledCapabilities: current.enabledCapabilities.filter((capability) => capability !== selectedCapability),
              }));
            }
          }}
        >
              <SegmentedControlItem value="provider_tool" aria-label={t("composer.capability.mode.provider")}>
                {t("composer.capability.mode.provider")}
              </SegmentedControlItem>
              <SegmentedControlItem value="model_replacement" aria-label={t("composer.capability.mode.model")}>
                {t("composer.capability.mode.model")}
              </SegmentedControlItem>
        </SegmentedControl>
      </label>
      {selectedMediaMode === "provider_tool" ? (
        <>
          <label>
            <span>{t("settings.providerTools.model")}</span>
            <Combobox
              ariaLabel={t("settings.providerTools.model")}
              placeholder={t("settings.providerTools.selectModel")}
              searchPlaceholder={t("settings.providerTools.selectModel")}
              emptyText={t("composer.capability.mediaBackendMissing")}
              options={selectedMediaProviderModels.map((target) => ({
                value: providerToolModelKey(target), label: target.target.display_name,
                keywords: [target.target.provider_id, target.connectionDisplayName, target.target.upstream_target_id],
              }))}
              disabled={saving || selectedMediaProviderModels.length === 0}
              value={selectedMediaProviderModelKey}
              onValueChange={(modelKey) => {
                const target = selectedMediaTargets.find((candidate) => (
                  providerToolModelKey(candidate) === modelKey
                  && candidate.connectionId === selectedMediaProviderTarget?.connectionId
                )) ?? selectedMediaTargets.find((candidate) => (
                  providerToolModelKey(candidate) === modelKey
                ));
                if (target) {
                  updateMediaSelection(selectedCapability, providerMediaUnderstandingSelection(target));
                }
              }}
            />
          </label>
          <label>
            <span>{t("settings.providerTools.account")}</span>
            <Select
              disabled={saving || selectedMediaProviderAccounts.length === 0}
              value={selectedMediaProviderTarget?.connectionId ?? ""}
              onValueChange={(connectionId) => {
                const target = selectedMediaProviderAccounts.find(
                  (candidate) => candidate.connectionId === connectionId,
                );
                if (target) {
                  updateMediaSelection(selectedCapability, providerMediaUnderstandingSelection(target));
                }
              }}
            >
              <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectAccount")} /></SelectTrigger>
              <SelectContent>
                {selectedMediaProviderAccounts.map((target) => (
                  <SelectItem key={target.connectionId} value={target.connectionId}>
                    {target.connectionDisplayName}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>
        </>
      ) : selectedMediaMode === "model_replacement" ? (
        <>
          <label>
            <span>{t("settings.providerTools.model")}</span>
            <Combobox
              ariaLabel={t("settings.providerTools.model")}
              placeholder={t("settings.providerTools.selectModel")}
              searchPlaceholder={t("settings.providerTools.selectModel")}
              emptyText={t("composer.capability.mediaBackendMissing")}
              options={selectedMediaModelChoices.map((candidate) => ({
                value: mediaUnderstandingModelKey(candidate), label: `${candidate.providerDisplayName} · ${candidate.modelDisplayName}`,
                keywords: [candidate.selection.provider_id, candidate.selection.model_id, candidate.connectionDisplayName],
              }))}
              disabled={saving || selectedMediaModelChoices.length === 0}
              value={selectedMediaModelKey}
              onValueChange={(modelKey) => {
                const candidate = selectedMediaCandidates.find(
                  (item) => mediaUnderstandingModelKey(item) === modelKey,
                );
                if (candidate) {
                  updateMediaSelection(selectedCapability, modelReplacementSelection(candidate));
                }
              }}
            />
          </label>
          <label>
            <span>{t("settings.providerTools.account")}</span>
            <Select
              disabled={saving || selectedMediaModelAccounts.length === 0}
              value={selectedMediaModelCandidate?.selection.connection_id ?? ""}
              onValueChange={(connectionId) => {
                const candidate = selectedMediaModelAccounts.find(
                  (item) => item.selection.connection_id === connectionId,
                );
                if (candidate) {
                  updateMediaSelection(selectedCapability, modelReplacementSelection(
                    candidate,
                    supportedMediaReplacementReasoningEffort(
                      candidate,
                      selectedMediaSelection?.mode === "model_replacement" ? selectedMediaSelection.reasoning_effort : undefined,
                    ),
                  ));
                }
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={t("settings.providerTools.selectAccount")} />
              </SelectTrigger>
              <SelectContent>
                {selectedMediaModelAccounts.map((candidate) => (
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
          {selectedMediaReasoningEfforts.length > 0 ? (
            <label>
              <span>{t("settings.providerTools.reasoningEffort")}</span>
              <Select
                disabled={saving || !selectedMediaModelCandidate}
                value={(selectedMediaSelection?.mode === "model_replacement" ? selectedMediaSelection.reasoning_effort : undefined) ?? "__default__"}
                onValueChange={(value) => {
                  if (!selectedMediaModelCandidate) return;
                  const effort = value === "__default__"
                    ? undefined
                    : value as PublicAiReasoningEffort;
                  updateMediaSelection(selectedCapability, modelReplacementSelection(selectedMediaModelCandidate, effort));
                }}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="__default__">
                    {t("settings.providerTools.modelDefaultReasoning")}
                  </SelectItem>
                  {selectedMediaReasoningEfforts.map((effort) => (
                    <SelectItem key={effort} value={effort}>
                      {t(PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS[effort])}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          ) : null}
        </>
      ) : (
        <div className="composer-capability-dialog-empty">
          {t("settings.providerTools.noSelection")}
        </div>
      )}
      <p className="composer-capability-media-note settings-muted-line">
        {t("composer.capability.mediaBackendNote")}
      </p>
      {(selectedMediaMode === "provider_tool" ? selectedMediaTargets.length : selectedMediaCandidates.length) === 0
        ? <p className="settings-muted-line" role="status">{t("composer.capability.mediaBackendMissing")}</p> : null}
    </div>
  );
}

// ComposerCapabilityExtensionList edits the native extension model set and its unavailable-entry recovery.
// ComposerCapabilityExtensionList 编辑原生扩展模型集合及其不可用条目恢复。
export function ComposerCapabilityExtensionList(props: {
  // Session capability state and actions from useComposerCapabilitySettings.
  // 来自 useComposerCapabilitySettings 的会话能力状态与操作。
  settings: ComposerCapabilitySettings;
  // Receives the list element so recovery focus can move to the next recovery action.
  // 接收列表元素，使恢复焦点可移动到下一个恢复操作。
  extensionRecoveryList: React.RefObject<HTMLDivElement | null>;
  // Marks that a recovery click removed its own button and focus must be restored.
  // 标记恢复点击移除了自身按钮，需要还原焦点。
  recoveryFocusPending: React.RefObject<boolean>;
}) {
  const { t } = useI18n();
  const { extensionRecoveryList, recoveryFocusPending } = props;
  const {
    selectedCapability,
    setDraft,
    saving,
    mutationPending,
    capabilityTargets,
    selectedModels,
    selectedExtensionTargets,
    unavailableExtensionTargets,
  } = props.settings;
  return (
    <div className="composer-capability-extension-list" ref={extensionRecoveryList}>
      <p className="settings-muted-line">
        {t("composer.capability.extensionHint")}
      </p>
      {unavailableExtensionTargets.map((selection) => (
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1 rounded-md border p-3"
          key={JSON.stringify([selection.connection_id, selection.target_id])}>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium" title={selection.target_id}>{selection.target_id}</p>
            <span className="text-xs text-muted-foreground">{t("settings.providerTools.unavailable")}</span>
          </div>
          <Button variant="outline" size="sm" disabled={saving} data-extension-recovery
            aria-label={`${t("composer.capability.removeUnavailable")} · ${selection.target_id}`}
            onClick={() => {
              if (mutationPending.current) return;
              recoveryFocusPending.current = true;
              setDraft((current) => ({ ...current, extensionTargetSets: {
                ...current.extensionTargetSets,
                [selectedCapability]: (current.extensionTargetSets[selectedCapability] ?? []).filter((saved) =>
                  saved.connection_id !== selection.connection_id || saved.target_id !== selection.target_id),
              } }));
            }}>
            {t("composer.capability.removeUnavailable")}
          </Button>
          <details className="col-span-2 min-w-0 text-xs text-muted-foreground">
            <summary className="cursor-pointer">{t("composer.capability.unavailableDetails")}</summary>
            <dl className="mt-2 space-y-1 break-all">
              <dt>{t("settings.providerTools.model")}</dt><dd>{selection.target_id}</dd>
              <dt>{t("settings.providerTools.account")}</dt><dd>{selection.connection_id}</dd>
            </dl>
          </details>
        </div>
      ))}
      {selectedModels.length === 0 && unavailableExtensionTargets.length === 0
        ? <p role="status" className="settings-muted-line">{t("composer.capability.empty.noTargets")}</p> : null}
      {selectedModels.map((model) => {
        // modelTargets contains every configured account for one extension model.
        // modelTargets 包含一个扩展模型的全部已配置账号。
        const modelTargets = capabilityTargets.filter((target) => (
          target.target.provider_id === model.target.provider_id
          && target.target.upstream_target_id === model.target.upstream_target_id
        ));
        // selectedExtension resolves the saved target for this model.
        // selectedExtension 解析当前模型已保存的目标。
        const selectedExtension = selectedExtensionTargets.find((selection) => (
          modelTargets.some((target) => (
            target.connectionId === selection.connection_id
            && target.target.target_id === selection.target_id
          ))
        ));
        // selectedExtensionTarget resolves the active account or the first candidate.
        // selectedExtensionTarget 解析活动账号或第一个候选账号。
        const selectedExtensionTarget = modelTargets.find((target) => (
          target.connectionId === selectedExtension?.connection_id
          && target.target.target_id === selectedExtension.target_id
        )) ?? modelTargets[0];
        // profile contains reviewed model behavior and constraints.
        // profile 包含已审核的模型行为与约束。
        const profile = model.target.contract.extension_profile;
        return (
          <ProviderToolExtensionModelCard
            accountControl={(
              <Select
                disabled={saving || !selectedExtension}
                onValueChange={(connectionId) => {
                  const replacement = modelTargets.find((target) => target.connectionId === connectionId);
                  if (!replacement) return;
                  const retained = selectedExtensionTargets.filter((selection) => !modelTargets.some((target) => (
                    target.connectionId === selection.connection_id
                    && target.target.target_id === selection.target_id
                  )));
                  retained.push(selectionForTarget(replacement));
                  setDraft((current) => ({
                    ...current,
                    extensionTargetSets: {
                      ...current.extensionTargetSets,
                      [selectedCapability]: retained,
                    },
                  }));
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
            characteristics={null}
            checked={Boolean(selectedExtension)}
            checkboxDisabled={saving || !profile}
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
              const retained = selectedExtensionTargets.filter((selection) => !modelTargets.some((target) => (
                target.connectionId === selection.connection_id
                && target.target.target_id === selection.target_id
              )));
              if (checked && selectedExtensionTarget) {
                retained.push(selectionForTarget(selectedExtensionTarget));
              }
              setDraft((current) => ({
                ...current,
                extensionTargetSets: {
                  ...current.extensionTargetSets,
                  [selectedCapability]: retained,
                },
              }));
            }}
            providerId={model.target.provider_id}
            summary={profile?.summary ?? t("composer.capability.extensionProfileMissing")}
            title={model.target.display_name}
          />
        );
      })}
    </div>
  );
}

// ComposerCapabilityDirectRoute selects the model and account for one direct provider-tool capability.
// ComposerCapabilityDirectRoute 为一项直连供应商工具能力选择模型与账号。
export function ComposerCapabilityDirectRoute(props: {
  // Session capability state and actions from useComposerCapabilitySettings.
  // 来自 useComposerCapabilitySettings 的会话能力状态与操作。
  settings: ComposerCapabilitySettings;
}) {
  const { t } = useI18n();
  const {
    selectedCapability,
    setDraft,
    saving,
    capabilityTargets,
    selectedModels,
    selectedModel,
    selectedAccounts,
    selectedAccount,
  } = props.settings;
  return (
    <div className="composer-capability-route-selectors">
      <label>
        <span>{t("settings.providerTools.model")}</span>
        <Combobox
          ariaLabel={t("settings.providerTools.model")}
          placeholder={t("settings.providerTools.selectModel")}
          searchPlaceholder={t("settings.providerTools.selectModel")}
          emptyText={t("composer.capability.empty.noTargets")}
          options={selectedModels.map((target) => ({ value: target.target.target_id, label: target.target.display_name,
            keywords: [target.target.provider_id, target.connectionDisplayName, target.target.upstream_target_id],
          }))}
          disabled={saving || selectedModels.length === 0}
          onValueChange={(targetId) => {
            const compatible = capabilityTargets.filter(
              (target) => target.target.target_id === targetId,
            );
            const target = compatible.find(
              (candidate) => candidate.connectionId === selectedAccount?.connectionId,
            ) ?? compatible[0];
            if (target) {
              setDraft((current) => ({
                ...current,
                selections: {
                  ...current.selections,
                  [selectedCapability]: selectionForTarget(target),
                },
              }));
            }
          }}
          value={selectedModel?.target.target_id ?? ""}
        />
      </label>
      <label>
        <span>{t("settings.providerTools.account")}</span>
        <Select
          disabled={saving || !selectedModel}
          onValueChange={(connectionId) => {
            const target = selectedAccounts.find(
              (candidate) => candidate.connectionId === connectionId,
            );
            if (target) {
              setDraft((current) => ({
                ...current,
                selections: {
                  ...current.selections,
                  [selectedCapability]: selectionForTarget(target),
                },
              }));
            }
          }}
          value={selectedAccount?.connectionId ?? ""}
        >
          <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectAccount")} /></SelectTrigger>
          <SelectContent>
            {selectedAccounts.map((target) => (
              <SelectItem key={target.connectionId} value={target.connectionId}>
                {target.connectionDisplayName}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      {selectedModels.length === 0 ? <p role="status" className="settings-muted-line">{t("composer.capability.empty.noTargets")}</p> : null}
    </div>
  );
}
