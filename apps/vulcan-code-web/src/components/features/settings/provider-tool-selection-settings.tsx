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
  savePublicAiProviderExtensionTargetSet,
  savePublicAiProviderToolCapabilityEnabled,
  savePublicAiProviderToolSelection,
} from "@/service/settings-actions-service";
import type {
  PublicAiProviderToolCapabilityKind,
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
  // extensionMode selects stable native multi-model configuration for generative capabilities.
  // extensionMode 为生成类能力选择稳定原生多模型配置。
  const extensionMode = isModelExtensionCapability(props.capability);
  // extensionSelections is the complete persisted target set for the current capability.
  // extensionSelections 是当前能力完整的持久化目标集合。
  const extensionSelections = snapshot?.providerExtensionTargetSets[props.capability] ?? [];
  const models = uniqueProviderToolModels(targets);
  const defaultSelection = snapshot?.providerToolSelections[props.capability];
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

  // saveSelection persists one exact model-account pair without changing the capability switch.
  // saveSelection 持久化一个精确模型账号组合且不修改能力开关。
  const saveSelection = React.useCallback(async (target: PublicAiProviderToolTargetSnapshot) => {
    setPending(true);
    try {
      const next = await savePublicAiProviderToolSelection({
        capability: props.capability,
        selection: {
          connection_id: target.connectionId,
          target_id: target.target.target_id,
        },
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPending(false);
    }
  }, [props.capability]);

  // saveEnabled persists only the system capability switch and initializes a missing default explicitly.
  // saveEnabled 仅持久化系统能力开关，并在缺少默认值时显式初始化。
  const saveEnabled = React.useCallback(async (nextEnabled: boolean) => {
    if (!snapshot) return;
    setPending(true);
    try {
      let next = snapshot;
      if (nextEnabled && !defaultSelection && selectedAccount) {
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
  }, [defaultSelection, props.capability, selectedAccount, snapshot]);

  return (
    <div className="settings-direct-content settings-capability-page">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot ? (
        <p className="settings-muted-line">{t("settings.providerTools.loading")}</p>
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

// uniqueProviderToolModels collapses account-expanded targets into stable model rows.
// uniqueProviderToolModels 将按账号展开的目标折叠为稳定模型行。
export function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    // modelKey prevents collisions between providers that reuse an upstream model id.
    // modelKey 防止复用上游模型标识的不同供应商发生冲突。
    const modelKey = `${target.target.provider_id}/${target.target.upstream_target_id}`;
    if (!models.has(modelKey)) models.set(modelKey, target);
  }
  return Array.from(models.values()).sort((left, right) => (
    left.target.display_name.localeCompare(right.target.display_name)
    || left.target.target_id.localeCompare(right.target.target_id)
  ));
}

// errorMessage converts a rejected value into a visible diagnostic.
// errorMessage 将被拒绝的值转换为可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
