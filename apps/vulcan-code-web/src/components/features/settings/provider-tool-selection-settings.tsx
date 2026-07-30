import * as React from "react";
import { FlaskConical, Star } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useI18n } from "@/i18n";
import {
  loadPublicAiSettings,
  savePublicAiProviderToolSelection,
  savePublicAiProviderToolTargetEnabled,
} from "@/service/settings-actions-service";
import type {
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSnapshot,
  PublicAiSettingsSnapshot,
} from "@/types";
import { ProviderToolTestDialog } from "./provider-tool-test-dialog";

// ProviderToolSelectionSettingsArea lists every current target and owns enabled/default preferences.
// ProviderToolSelectionSettingsArea 列出全部当前目标并管理启用状态与默认目标。
export function ProviderToolSelectionSettingsArea(props: {
  // Semantic capability configured by this settings page.
  // 当前设置页面配置的语义能力。
  capability: PublicAiProviderToolCapabilityKind;
}) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [pendingKey, setPendingKey] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  // testTarget is the exact current credential and provider target under diagnosis.
  // testTarget 是当前正在诊断的精确凭据与供应商目标。
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
        if (active) {
          setError(errorMessage(reason));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const targets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === props.capability,
  ) ?? [];
  const defaultSelection = snapshot?.providerToolSelections[props.capability];

  // saveEnabled persists one row without hiding the complete live target catalog.
  // saveEnabled 持久化单行启用状态，同时保留完整实时目标目录展示。
  const saveEnabled = React.useCallback(async (
    target: PublicAiProviderToolTargetSnapshot,
    enabled: boolean,
  ) => {
    const key = providerToolTargetKey(target);
    setPendingKey(key);
    try {
      const next = await savePublicAiProviderToolTargetEnabled({
        capability: props.capability,
        selection: {
          connection_id: target.connectionId,
          target_id: target.target.target_id,
        },
        enabled,
      });
      setSnapshot(next);
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setPendingKey(null);
    }
  }, [props.capability]);

  // saveDefault makes one enabled current target the capability default.
  // saveDefault 将一个已启用的当前目标设为能力默认目标。
  const saveDefault = React.useCallback(async (
    target: PublicAiProviderToolTargetSnapshot,
  ) => {
    const key = providerToolTargetKey(target);
    setPendingKey(key);
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
      setPendingKey(null);
    }
  }, [props.capability]);

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
        <div className="settings-capability-route-list">
          {targets.map((target) => {
            const key = providerToolTargetKey(target);
            const isDefault = defaultSelection?.connection_id === target.connectionId
              && defaultSelection.target_id === target.target.target_id;
            return (
              <article className="settings-capability-route-row provider-tool-settings-row" key={key}>
                <span className="settings-route-main">
                  <strong>{target.target.display_name}</strong>
                  <small>
                    {providerDisplayName(snapshot, target.target.provider_id)}
                    {" · "}
                    {target.connectionDisplayName}
                  </small>
                </span>
                <button
                  aria-label={t("settings.providerTools.setDefault")}
                  className={`skill-action-button icon compact ${isDefault ? "active" : "secondary"}`}
                  disabled={!target.enabled || pendingKey === key}
                  onClick={() => void saveDefault(target)}
                  title={isDefault
                    ? t("settings.providerTools.default")
                    : t("settings.providerTools.setDefault")}
                  type="button"
                >
                  <Star fill={isDefault ? "currentColor" : "none"} size={15} />
                </button>
                {props.capability === "web_search" || props.capability === "web_extract" ? (
                  <Button
                    aria-label={t("settings.providerTools.test")}
                    disabled={pendingKey === key}
                    onClick={() => setTestTarget(target)}
                    size="icon-sm"
                    title={t("settings.providerTools.test")}
                    type="button"
                    variant="outline"
                  >
                    <FlaskConical aria-hidden="true" size={15} />
                  </Button>
                ) : null}
                <Switch
                  aria-label={target.enabled
                    ? t("settings.providerTools.enabled")
                    : t("settings.providerTools.disabled")}
                  checked={target.enabled}
                  disabled={pendingKey === key}
                  onCheckedChange={(enabled) => void saveEnabled(target, enabled)}
                />
              </article>
            );
          })}
        </div>
      )}
      <ProviderToolTestDialog
        target={testTarget}
        onClose={() => setTestTarget(null)}
      />
    </div>
  );
}

// providerToolTargetKey creates a collision-resistant UI identity from exact wire fields.
// providerToolTargetKey 从精确线协议字段创建抗冲突 UI 标识。
function providerToolTargetKey(target: PublicAiProviderToolTargetSnapshot): string {
  return `${target.connectionId.length}:${target.connectionId}:${target.target.target_id.length}:${target.target.target_id}`;
}

// providerDisplayName resolves the registered display name for one current provider.
// providerDisplayName 解析一个当前供应商的注册显示名称。
function providerDisplayName(snapshot: PublicAiSettingsSnapshot, providerId: string): string {
  return snapshot.providers.find((provider) => provider.id === providerId)?.display_name
    ?? providerId;
}

// errorMessage converts a rejected value into a visible diagnostic.
// errorMessage 将被拒绝的值转换为可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
