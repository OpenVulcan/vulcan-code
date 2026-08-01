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
import {
  loadPublicAiSettings,
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
    (target) => target.target.contract.capability === props.capability,
  ) ?? [];
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
    if (!models.has(target.target.target_id)) models.set(target.target.target_id, target);
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
